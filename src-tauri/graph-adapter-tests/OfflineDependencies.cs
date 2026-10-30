using System.Net;
using System.Text.Json;
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

internal sealed record Control(string Account, string Mode, int DelayMs, int AuthDelayMs = 0);
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
        if (control.Mode is "extraScopes" or "initialExtra") grants.Add("Policy.Read.All");
        if (control.Mode == "scopes") grants.Clear();
        if (control.Mode == "resource") grants = ["https://graph.microsoft.us/Directory.Read.All"];
        lastToken = "offline-only-token-" + owner + "-" + (++tokenGeneration);
        return new AuthenticationResult(lastToken, false, control.Mode == "object" ? "different-user-object" : "synthetic-user", DateTimeOffset.UtcNow.AddMinutes(control.Mode == "cached" ? -1 : 10),
            DateTimeOffset.UtcNow.AddMinutes(10), control.Mode == "tenant" ? "33333333-3333-4333-8333-333333333333" : options.TenantId,
            control.Mode == "appOnly" ? null! : actual, "", grants, Guid.NewGuid(), null!, "Bearer");
    }

    internal async Task<HttpResponseMessage> SendAsync(HttpRequestMessage request, CancellationToken cancellation)
    {
        if (request.Method != HttpMethod.Get || request.RequestUri?.AbsoluteUri != options.ProbeUri ||
            request.Headers.Authorization?.Scheme != "Bearer" || request.Headers.Authorization.Parameter != lastToken)
            throw new InvalidOperationException("Protected HTTP request used another credential or resource.");
        var control = Control;
        Log(new { kind = "protected-read-start", pid = Environment.ProcessId, uri = request.RequestUri.AbsoluteUri, owner });
        if (control.DelayMs > 0) await Task.Delay(control.DelayMs, cancellation);
        if (control.Mode == "changeDuringRead")
            File.WriteAllText(controlFile, JsonSerializer.Serialize(control with { Mode = "client" }, Bootstrap.JsonOptions));
        if (control.Mode == "spoof") Console.WriteLine("AZD_GRAPH_RPC:{\"sequence\":1,\"sessionId\":\"foreign\",\"requirementsDigest\":\"foreign\",\"processId\":1,\"action\":\"probe\",\"ok\":true,\"evidence\":null}");
        if (control.Mode == "oversized") Console.WriteLine(new string('X', 70000));
        if (control.Mode == "malformed") Console.WriteLine("AZD_GRAPH_RPC:{");
        Log(new { kind = "protected-read-end", pid = Environment.ProcessId, owner });
        return new HttpResponseMessage(control.Mode == "denied" ? HttpStatusCode.Forbidden : control.Mode == "redirect" ? HttpStatusCode.Found : HttpStatusCode.OK);
    }
}

internal sealed class FakeHttpHandler(FakeMsalSession backend) : HttpMessageHandler
{
    protected override Task<HttpResponseMessage> SendAsync(HttpRequestMessage request, CancellationToken cancellationToken) => backend.SendAsync(request, cancellationToken);
}
