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

    public static int Run(string root)
    {
        int checks = 0;
        foreach (var uri in new[] { "https://graph.microsoft.com/v1.0/users", Next, Initial + "&$expand=roleAssignments", "http://graph.microsoft.com/beta/roleManagement/directory/roleDefinitions" })
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
        foreach (var status in new[] { HttpStatusCode.Forbidden, HttpStatusCode.Found })
        {
            using var fixture = new Fixture(root, _ => new HttpResponseMessage(status));
            Reject(() => fixture.Session.ReadPimJson(Initial));
            Check(fixture.Http.Calls == 1, "Denied response was replayed.");
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
        return checks;
    }
}
