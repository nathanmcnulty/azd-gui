using System.Net;
using System.Text.Json;
using AzdGui.GraphAdapter;

namespace AzdGui.GraphAdapter.Tests;

// Runs the production managed reader against offline public-SDK result objects
// and bounded HTTP fixtures. This assembly is never embedded in the desktop app.
public static class PimReaderChecks
{
    private const string Initial = "https://graph.microsoft.com/beta/roleManagement/directory/roleDefinitions?$filter=isPrivileged%20eq%20true";
    private const string Next = Initial + "&$skiptoken=fixture";
    private static void Check(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }
    private static void Reject(Action action)
    {
        bool rejected = false;
        try { action(); } catch { rejected = true; }
        Check(rejected, "Expected managed reader rejection.");
    }
    private sealed class Handler(Func<int, HttpResponseMessage> respond) : HttpMessageHandler
    {
        public int Calls { get; private set; }
        protected override Task<HttpResponseMessage> SendAsync(HttpRequestMessage request, CancellationToken cancellationToken)
        {
            Check(request.Method == HttpMethod.Get, "Reader attempted a write.");
            Check(request.Headers.Authorization?.Scheme == "Bearer", "Reader omitted its managed authorization.");
            if (request.RequestUri?.AbsoluteUri == "https://graph.microsoft.com/v1.0/identity/conditionalAccess/policies?$top=1&$select=id")
                return Task.FromResult(Json("{\"value\":[]}"));
            return Task.FromResult(respond(++Calls));
        }
    }
    private sealed class Fixture : IDisposable
    {
        public OwnedGraphSession Session { get; }
        public Handler Http { get; }
        private readonly string control;
        public Fixture(string root, Func<int, HttpResponseMessage> response, string[]? scopes = null)
        {
            var directory = Path.Combine(root, Guid.NewGuid().ToString());
            Directory.CreateDirectory(directory);
            control = Path.Combine(directory, "control.json");
            Mode("good");
            var options = new SessionOptions("offline-fixture", "operator@example.test",
                "11111111-1111-4111-8111-111111111111", "22222222-2222-4222-8222-222222222222",
                "https://graph.microsoft.com", scopes ?? ["RoleManagement.Read.Directory", "Policy.Read.All", "AuthenticationContext.Read.All"],
                "https://graph.microsoft.com/v1.0/identity/conditionalAccess/policies?$top=1&$select=id", "pimPlan");
            Http = new Handler(response);
            Session = new OwnedGraphSession(options, new FakeMsalSession(options, control), new HttpClient(Http));
            Session.Authenticate();
            Session.Probe();
        }
        public void Mode(string mode) => File.WriteAllText(control, JsonSerializer.Serialize(new Control("operator@example.test", mode, 0), Bootstrap.JsonOptions));
        public void Dispose() => Session.Dispose();
    }
    private static HttpResponseMessage Json(string body) => new(HttpStatusCode.OK) { Content = new StringContent(body) };

    private sealed class ErrorBodyTripwire : HttpContent
    {
        public bool WasRead { get; private set; }
        protected override Task SerializeToStreamAsync(Stream stream, TransportContext? context)
        {
            WasRead = true;
            throw new InvalidOperationException("offline-private-error-body-sentinel");
        }
        protected override bool TryComputeLength(out long length) { length = 0; return false; }
    }

    public static int Run(string root)
    {
        int checks = 0;
        foreach (var uri in new[] { "https://graph.microsoft.com/v1.0/users", Next, Initial + "&$expand=roleAssignments", "http://graph.microsoft.com/beta/roleManagement/directory/roleDefinitions", "https://graph.microsoft.com/v1.0/policies/authenticationStrengthPolicies?$top=100" })
        {
            using var fixture = new Fixture(root, _ => Json("{\"value\":[]}"));
            Reject(() => fixture.Session.ReadPimJson(uri));
            Check(fixture.Http.Calls == 0, "Unregistered URI dispatched HTTP.");
            checks++;
        }
        foreach (var uri in new[] {
            "https://example.invalid/beta/roleManagement/directory/roleDefinitions?$skiptoken=x",
            "http://graph.microsoft.com/beta/roleManagement/directory/roleDefinitions?$skiptoken=x",
            "https://graph.microsoft.com:444/beta/roleManagement/directory/roleDefinitions?$skiptoken=x",
            "https://user@graph.microsoft.com/beta/roleManagement/directory/roleDefinitions?$skiptoken=x",
            Next + "#fragment", "https://graph.microsoft.com/v1.0/users?$skiptoken=x",
            Initial + "&$expand=roleAssignments&$skiptoken=x", Initial.Replace("true", "false") + "&$skiptoken=x",
            Initial + "&$skiptoken=x&$skiptoken=y"
        })
        {
            using var fixture = new Fixture(root, _ => Json(JsonSerializer.Serialize(new Dictionary<string, object> { ["value"] = Array.Empty<object>(), ["@odata.nextLink"] = uri })));
            Reject(() => fixture.Session.ReadPimJson(Initial));
            Reject(() => fixture.Session.ReadPimJson(Initial));
            Check(fixture.Http.Calls == 1, "Invalid continuation left reusable reader.");
            checks++;
        }
        foreach (var body in new[] { "{invalid", "{\"value\":{}}", "{\"unexpected\":[]}", "[]", "{\"value\":" + new string('[', 66) + "0" + new string(']', 66) + "}", "{\"value\":[\"" + new string('x', 4 * 1024 * 1024) + "\"]}" })
        {
            using var fixture = new Fixture(root, _ => Json(body));
            Reject(() => fixture.Session.ReadPimJson(Initial));
            Check(fixture.Http.Calls == 1, "Malformed response dispatched extra requests.");
            checks++;
        }
        foreach (var (status, expectedCode) in new (int, string)[] {
            (400, "GraphReadBadRequest"), (401, "GraphReadDenied"), (403, "GraphReadDenied"),
            (404, "GraphReadNotFound"), (429, "GraphReadThrottled"),
            (300, "GraphRedirectRejected"), (302, "GraphRedirectRejected"), (307, "GraphRedirectRejected"), (399, "GraphRedirectRejected"),
            (409, "GraphReadFailed"), (500, "GraphReadFailed"), (503, "GraphReadFailed")
        })
        {
            var content = new ErrorBodyTripwire();
            using var fixture = new Fixture(root, _ => new HttpResponseMessage((HttpStatusCode)status) {
                ReasonPhrase = "offline-private-reason-sentinel", Content = content,
                Headers = { Location = new Uri("https://example.invalid/offline-private-location-sentinel") }
            });
            GraphBoundaryException? failure = null;
            try { fixture.Session.ReadPimJson(Initial); } catch (GraphBoundaryException exception) { failure = exception; }
            Check(failure is not null && failure.Code == expectedCode && failure.Message == expectedCode, "HTTP failure did not use its closed safe category.");
            Check(!content.WasRead, "HTTP failure body was read.");
            Reject(() => fixture.Session.ReadPimJson(Initial));
            Check(fixture.Http.Calls == 1, "Failed response was replayed or redirected.");
            checks++;
        }
        {
            using var fixture = new Fixture(root, n => Json(n == 1 ? JsonSerializer.Serialize(new Dictionary<string, object> { ["value"] = Array.Empty<object>(), ["@odata.nextLink"] = Next }) : "{\"value\":[]}"));
            fixture.Session.ReadPimJson(Initial);
            fixture.Session.ReadPimJson(Next);
            Reject(() => fixture.Session.ReadPimJson(Next));
            Check(fixture.Http.Calls == 2, "Consumed continuation was replayed.");
            checks++;
        }
        {
            using var fixture = new Fixture(root, _ => Json("{\"value\":[]}"));
            for (int i = 0; i < 100; i++) fixture.Session.ReadPimJson(Initial);
            Reject(() => fixture.Session.ReadPimJson(Initial));
            Check(fixture.Http.Calls == 100, "Request limit exceeded.");
            checks++;
        }
        {
            var body = "{\"value\":[\"" + new string('x', 3 * 1024 * 1024) + "\"]}";
            using var fixture = new Fixture(root, _ => Json(body));
            for (int i = 0; i < 5; i++) fixture.Session.ReadPimJson(Initial);
            Reject(() => fixture.Session.ReadPimJson(Initial));
            Reject(() => fixture.Session.ReadPimJson(Initial));
            Check(fixture.Http.Calls == 6, "Aggregate response limit did not close reader.");
            checks++;
        }
        foreach (var mode in new[] { "account", "tenant", "principal", "scopes", "extraScopes", "client", "resource", "silentRequired" })
        {
            using var fixture = new Fixture(root, _ => Json(JsonSerializer.Serialize(new Dictionary<string, object> { ["value"] = Array.Empty<object>(), ["@odata.nextLink"] = Next })));
            fixture.Session.ReadPimJson(Initial);
            fixture.Mode(mode);
            Reject(() => fixture.Session.ReadPimJson(Next));
            Check(fixture.Http.Calls == 1, "Changed credential dispatched a later-page read.");
            checks++;
        }
        {
            using var fixture = new Fixture(root, _ => Json("{\"value\":[]}"), ["Policy.Read.All"]);
            Reject(() => fixture.Session.ReadPimJson(Initial));
            Check(fixture.Http.Calls == 0, "Unrequested scope dispatched a read.");
            checks++;
        }
        {
            using var fixture = new Fixture(root, _ => Json("{\"value\":[]}"));
            var evidence = JsonSerializer.Deserialize<GraphObservation>(fixture.Session.EvidenceJson(), Bootstrap.JsonOptions)!;
            Check(evidence.VerificationReads == 1 && evidence.PimReadsAttempted == 0 && evidence.PrincipalFingerprint?.Length == 64, "Verification receipt missing actual process evidence.");
            Reject(() => fixture.Session.Probe());
            Check(fixture.Http.Calls == 0, "Repeated verification dispatched a PIM read.");
            checks++;
        }
        {
            using var fixture = new Fixture(root, _ => Json("{\"value\":[]}"));
            fixture.Session.ReadPimJson(Initial);
            var evidence = JsonSerializer.Deserialize<GraphObservation>(fixture.Session.EvidenceJson(), Bootstrap.JsonOptions)!;
            Check(evidence.PimReadsAttempted == 1 && evidence.PimReadsCompleted == 1 && evidence.VerificationReads == 1, "Read counters did not reflect completed protected dispatch.");
            checks++;
        }
        // Real catalog join: stable metadata paginates, custom and unknown roles
        // remain visible, and later-page failure yields no usable result.
        const string roleId = "33333333-3333-4333-8333-333333333333";
        const string customId = "44444444-4444-4444-8444-444444444444";
        const string unknownId = "55555555-5555-4555-8555-555555555555";
        string Row(string id, string name, bool builtIn) => JsonSerializer.Serialize(new { id, displayName = name, isBuiltIn = builtIn, isEnabled = true });
        string Page(params string[] rows) => "{\"value\":[" + string.Join(",", rows) + "]}";
        var metadataRow = Row(roleId, "Microsoft role", true);
        foreach (var mode in new[] { "good", "denied", "partial", "duplicate", "unknown", "empty", "classificationConflict", "cycle" })
        {
            using var fixture = new Fixture(root, n =>
            {
                if (mode == "denied" && n == 1) return new HttpResponseMessage(HttpStatusCode.Forbidden);
                if (mode == "empty") return Json(Page());
                if (n == 1) return Json("{\"value\":[" + metadataRow + "],\"@odata.nextLink\":\"" + OwnedGraphSession.RoleDefinitionsUri + "?$skiptoken=page2\"}");
                if (mode == "partial" && n == 2) return new HttpResponseMessage(HttpStatusCode.ServiceUnavailable);
                if (mode == "cycle" && n == 2) return Json("{\"value\":[],\"@odata.nextLink\":\"" + OwnedGraphSession.RoleDefinitionsUri + "?$skiptoken=page2\"}");
                if (n == 2) return Json(mode == "duplicate" ? Page(metadataRow) : Page(Row(customId, "Custom role", false), Row(unknownId, "Unknown role", false)));
                if (n == 3) return Json("{\"value\":[{\"id\":\"" + roleId + "\",\"isPrivileged\":true}]}");
                return Json(mode == "classificationConflict" ? "{\"value\":[{\"id\":\"" + roleId + "\",\"isPrivileged\":false}]}" : mode == "unknown" ? Page() : "{\"value\":[{\"id\":\"" + customId + "\",\"isPrivileged\":false}]}");
            });
            if (mode is "denied" or "partial" or "duplicate" or "classificationConflict" or "cycle") Reject(() => fixture.Session.ReadRoleCatalogJson());
            else
            {
                using var catalog = JsonDocument.Parse(fixture.Session.ReadRoleCatalogJson());
                Check(catalog.RootElement.GetArrayLength() == (mode == "empty" ? 0 : 3), "Catalog omitted stable roles.");
                if (mode != "empty") {
                    var custom = catalog.RootElement.EnumerateArray().Single(role => role.GetProperty("id").GetString() == customId);
                    Check(!custom.GetProperty("isBuiltIn").GetBoolean(), "Custom role was reclassified as Microsoft built-in.");
                    Check(custom.GetProperty("isPrivileged").ValueKind == (mode == "unknown" ? JsonValueKind.Null : JsonValueKind.False), "Unknown classification became unprivileged.");
                    Check(catalog.RootElement.EnumerateArray().Single(role => role.GetProperty("id").GetString() == unknownId).GetProperty("isPrivileged").ValueKind == JsonValueKind.Null, "Unmatched role omitted or classified.");
                }
            }
            checks++;
        }
        return checks;
    }
}
