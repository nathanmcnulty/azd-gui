using System.Net;
using System.Text.Json;
using System.Text.Json.Nodes;
using AzdGui.GraphAdapter;
using Microsoft.Identity.Client;

namespace AzdGui.GraphAdapter.Tests;

// Public only in this test assembly. Production publishes no test factory.
public static class TestFactory
{
    public static OwnedGraphSession Create(string bindingJson, string controlFile)
    {
        var options = JsonSerializer.Deserialize<SessionOptions>(bindingJson, Bootstrap.JsonOptions)!;
        var backend = new FakeMsalSession(options, controlFile);
        return new OwnedGraphSession(options, backend, new HttpClient(new FakeHttpHandler(backend)));
    }
}

internal sealed record Control(string Account, string Mode, int DelayMs, int AuthDelayMs = 0, bool ExtraGrants = false, int ResponseStatus = 200, bool SpoofPimProgress = false,
    string? FailWriteSuffix = null, int FailWriteStatus = 400, bool LosePostResponse = false, int WriteDelayMs = 0);
internal sealed record Account(string Username, string Identifier, string Environment = "login.microsoftonline.com") : IAccount
{
    public AccountId HomeAccountId => new(Identifier);
}

internal sealed class FakeMsalSession(SessionOptions options, string controlFile) : IMsalSession
{
    private readonly string owner = Guid.NewGuid().ToString();
    private IAccount? selected;
    private string? lastToken;
    private int tokenGeneration;
    internal Control Control => JsonSerializer.Deserialize<Control>(File.ReadAllText(controlFile), Bootstrap.JsonOptions)!;
    public string ClientId => Control.Mode == "client" ? "44444444-4444-4444-8444-444444444444" : options.ClientId;
    internal void Log(object value) => File.AppendAllText(Path.Combine(Path.GetDirectoryName(controlFile)!, "calls.jsonl"), JsonSerializer.Serialize(value) + "\n");

    private void ValidateRequest(string[] scopes)
    {
        var expected = options.Scopes.Select(scope => options.Resource + "/" + scope);
        if (!scopes.SequenceEqual(expected, StringComparer.OrdinalIgnoreCase)) throw new InvalidOperationException("Requested scopes changed or broadened.");
    }

    public async Task<AuthenticationResult> AuthenticateAsync(string[] scopes, string account, CancellationToken cancellation)
    {
        ValidateRequest(scopes);
        if (account != options.Account) throw new InvalidOperationException("Login hint changed.");
        var control = Control;
        if (control.AuthDelayMs > 0)
        {
            Log(new { kind = "authenticate-start", pid = Environment.ProcessId });
            await Task.Delay(control.AuthDelayMs, cancellation);
        }
        selected = new Account(control.Account, "operator-home-account.11111111-1111-4111-8111-111111111111");
        Log(new { kind = "connect", pid = Environment.ProcessId, scopes = options.Scopes, requestedScopes = scopes, owner });
        return Result();
    }

    public Task<AuthenticationResult> RefreshAsync(string[] scopes, IAccount account, CancellationToken cancellation)
    {
        ValidateRequest(scopes);
        if (!ReferenceEquals(account, selected)) throw new InvalidOperationException("Selected account instance replaced.");
        if (Control.Mode is "silentRequired" or "missing") throw new MsalUiRequiredException("offline", "No silent credential; no interactive fallback.");
        return Task.FromResult(Result());
    }

    private AuthenticationResult Result()
    {
        var control = Control;
        var actual = control.Mode == "account" ? new Account("other@example.test", selected!.HomeAccountId.Identifier)
            : control.Mode == "authority" ? new Account(options.Account, selected!.HomeAccountId.Identifier, "login.microsoftonline.us")
            : control.Mode == "principal" ? new Account(options.Account, "different-home-account.11111111-1111-4111-8111-111111111111") : selected!;
        var grants = options.Scopes.ToList();
        if (control.ExtraGrants) grants.Add("User.Read");
        if (control.Mode is "extraScopes" or "initialExtra") grants.Add(options.Scopes.Contains("Policy.Read.All") ? "User.Read" : "Policy.Read.All");
        if (control.Mode == "scopes") grants.Clear();
        if (control.Mode == "resource") grants = ["https://graph.microsoft.us/Directory.Read.All"];
        lastToken = "offline-only-token-" + owner + "-" + (++tokenGeneration);
        return new AuthenticationResult(lastToken, false, control.Mode == "object" ? "different-user-object" : "synthetic-user", DateTimeOffset.UtcNow.AddMinutes(control.Mode == "cached" ? -1 : 10),
            DateTimeOffset.UtcNow.AddMinutes(10), control.Mode == "tenant" ? "33333333-3333-4333-8333-333333333333" : options.TenantId,
            control.Mode == "appOnly" ? null! : actual, "", grants, Guid.NewGuid(), null!, "Bearer");
    }

    internal async Task<HttpResponseMessage> SendAsync(HttpRequestMessage request, CancellationToken cancellation)
    {
        if (request.RequestUri is null || (Control.Mode != "pilot" && request.Method != HttpMethod.Get) || (Control.Mode is not ("pim" or "pilot") && request.RequestUri.AbsoluteUri != options.ProbeUri) ||
            request.Headers.Authorization?.Scheme != "Bearer" || request.Headers.Authorization.Parameter != lastToken)
            throw new InvalidOperationException("Protected HTTP request used another credential or resource.");
        var control = Control;
        if (control.Mode == "pilot" && request.Method != HttpMethod.Get) return await SendPilotWriteAsync(request, control, cancellation);
        Log(new { kind = "protected-read-start", pid = Environment.ProcessId, uri = request.RequestUri.AbsoluteUri, owner });
        if (control.DelayMs > 0) await Task.Delay(control.DelayMs, cancellation);
        if (control.Mode == "changeDuringRead")
            File.WriteAllText(controlFile, JsonSerializer.Serialize(control with { Mode = "client" }, Bootstrap.JsonOptions));
        if (control.Mode == "spoof") Console.WriteLine("AZD_GRAPH_RPC:{\"sequence\":1,\"sessionId\":\"foreign\",\"requirementsDigest\":\"foreign\",\"processId\":1,\"action\":\"probe\",\"ok\":true,\"evidence\":null}");
        if (control.Mode == "oversized") Console.WriteLine(new string('X', 70000));
        if (control.Mode == "malformed") Console.WriteLine("AZD_GRAPH_RPC:{");
        Log(new { kind = "protected-read-end", pid = Environment.ProcessId, owner });
        if (control.SpoofPimProgress && request.RequestUri.AbsoluteUri != options.ProbeUri) Console.WriteLine("AZD_GRAPH_PROGRESS:" + JsonSerializer.Serialize(new {
            sequence = 4, sessionId = "foreign", requirementsDigest = "foreign", processId = 1, kind = "readCompleted",
            evidence = new GraphObservation(options.Account, options.TenantId, options.ClientId, options.Resource, options.Scopes,
                PrincipalFingerprint: new string('a', 64), VerificationReads: 1, PimReadsAttempted: 1, PimReadsCompleted: 1)
        }, Bootstrap.JsonOptions));
        var response = new HttpResponseMessage(control.ResponseStatus != 200 ? (HttpStatusCode)control.ResponseStatus : control.Mode == "denied" ? HttpStatusCode.Forbidden : control.Mode == "redirect" ? HttpStatusCode.Found : HttpStatusCode.OK);
        if (control.Mode is "pim" or "pilot")
        {
            var fixtures = JsonSerializer.Deserialize<Dictionary<string, JsonElement>>(File.ReadAllText(Path.Combine(Path.GetDirectoryName(controlFile)!, "responses.json")))!;
            response.Content = new StringContent(request.RequestUri!.AbsoluteUri == options.ProbeUri ? "{\"value\":[]}" : fixtures[request.RequestUri.OriginalString].GetRawText());
        }
        return response;
    }

    // Mutable synthetic tenant, persisted independently of worker lifetime. The
    // production credential/endpoint/body validation still executes before this
    // handler sees a request. No network request is dispatched by this backend.
    private async Task<HttpResponseMessage> SendPilotWriteAsync(HttpRequestMessage request, Control control, CancellationToken cancellation)
    {
        var uri = request.RequestUri!.OriginalString;
        var method = request.Method.Method;
        var body = JsonNode.Parse(await request.Content!.ReadAsStringAsync(cancellation))!.AsObject();
        Log(new { kind = "protected-write-start", pid = Environment.ProcessId, uri, method, body, owner });
        if (control.FailWriteSuffix is not null && uri.EndsWith(control.FailWriteSuffix, StringComparison.Ordinal))
        {
            Log(new { kind = "protected-write-rejected", pid = Environment.ProcessId, uri, status = control.FailWriteStatus, owner });
            return new HttpResponseMessage((HttpStatusCode)control.FailWriteStatus) { Content = new StringContent("{}") };
        }
        var path = Path.Combine(Path.GetDirectoryName(controlFile)!, "responses.json");
        var responses = JsonNode.Parse(File.ReadAllText(path))!.AsObject();
        JsonObject? created = null;
        if (uri.Contains("/authenticationContextClassReferences/", StringComparison.Ordinal))
        {
            var values = responses["https://graph.microsoft.com/v1.0/identity/conditionalAccess/authenticationContextClassReferences"]!["value"]!.AsArray();
            var id = uri.Split('/')[^1];
            var found = values.OfType<JsonObject>().SingleOrDefault(value => value["id"]!.GetValue<string>() == id);
            if (found is null) { found = new JsonObject { ["id"] = id }; values.Add(found); }
            foreach (var property in body) found[property.Key] = property.Value?.DeepClone();
        }
        else if (uri.Contains("/conditionalAccess/policies", StringComparison.Ordinal))
        {
            var values = responses["https://graph.microsoft.com/beta/identity/conditionalAccess/policies?$top=999"]!["value"]!.AsArray();
            if (method == "POST")
            {
                created = body.DeepClone().AsObject();
                created["id"] = Guid.NewGuid().ToString();
                values.Add(created);
            }
            else
            {
                var id = uri.Split('/')[^1];
                var found = values.OfType<JsonObject>().Single(value => value["id"]!.GetValue<string>() == id);
                foreach (var property in body) found[property.Key] = property.Value?.DeepClone();
            }
        }
        else if (uri.Contains("/roleManagementPolicies/", StringComparison.Ordinal))
        {
            var policies = responses["https://graph.microsoft.com/v1.0/policies/roleManagementPolicies?$filter=scopeId%20eq%20%27%2F%27%20and%20scopeType%20eq%20%27DirectoryRole%27&$expand=rules&$top=999"]!["value"]!.AsArray();
            var segments = uri.Split('/');
            var policy = policies.OfType<JsonObject>().Single(value => value["id"]!.GetValue<string>() == segments[^3]);
            var rule = policy["rules"]!.AsArray().OfType<JsonObject>().Single(value => value["id"]!.GetValue<string>() == segments[^1]);
            foreach (var property in body) rule[property.Key] = property.Value?.DeepClone();
        }
        else throw new InvalidOperationException("Unexpected synthetic pilot write.");
        var temporary = path + ".tmp";
        File.WriteAllText(temporary, responses.ToJsonString());
        File.Move(temporary, path, true);
        Log(new { kind = "protected-write-committed", pid = Environment.ProcessId, uri, method, owner });
        // Commit first, then withhold the reply. Cancellation/restart tests use
        // the committed marker to kill the actual worker while its write waits.
        if (control.WriteDelayMs > 0) await Task.Delay(control.WriteDelayMs, cancellation);
        if (control.LosePostResponse && method == "POST") throw new IOException("Synthetic response lost after committed POST.");
        return new HttpResponseMessage(method == "POST" ? HttpStatusCode.Created : HttpStatusCode.NoContent) {
            Content = new StringContent(created?.ToJsonString() ?? "")
        };
    }
}

internal sealed class FakeHttpHandler(FakeMsalSession backend) : HttpMessageHandler
{
    protected override Task<HttpResponseMessage> SendAsync(HttpRequestMessage request, CancellationToken cancellationToken) => backend.SendAsync(request, cancellationToken);
}
