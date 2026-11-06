using System.Net.Http.Headers;
using System.Reflection;
using System.Runtime.CompilerServices;
using System.Runtime.Loader;
using System.Text.Json;
using Microsoft.Identity.Client;

[assembly: InternalsVisibleTo("AzdGui.GraphAdapter.Tests")]
namespace AzdGui.GraphAdapter;

public static class Bootstrap
{
    public static void LoadDependencies(string directory)
    {
        var root = Path.GetFullPath(directory);
        AssemblyLoadContext.Default.Resolving += (_, name) =>
        {
            var file = Path.Combine(root, name.Name + ".dll");
            return File.Exists(file) ? AssemblyLoadContext.Default.LoadFromAssemblyPath(file) : null;
        };
    }

    public static OwnedGraphSession Create(string bindingJson) => new(
        JsonSerializer.Deserialize<SessionOptions>(bindingJson, JsonOptions) ?? throw new InvalidOperationException("Graph binding absent."));

    internal static readonly JsonSerializerOptions JsonOptions = new() { PropertyNamingPolicy = JsonNamingPolicy.CamelCase, PropertyNameCaseInsensitive = true };
}

public sealed record SessionOptions(string RequirementsDigest, string Account, string TenantId, string ClientId, string Resource, string[] Scopes, string ProbeUri);
public sealed record GraphObservation(string Account, string TenantId, string ClientId, string Resource, string[] Scopes, string AuthType = "Delegated", string ContextScope = "Process", string Environment = "Global");

// This seam replaces public SDK/network dependencies in offline tests, while
// exercising the same result validation and protected HTTP dispatch as production.
internal interface IMsalSession
{
    string ClientId { get; }
    Task<AuthenticationResult> AuthenticateAsync(string[] scopes, string account, CancellationToken cancellation);
    Task<AuthenticationResult> RefreshAsync(string[] scopes, IAccount account, CancellationToken cancellation);
}

internal sealed class MsalSession : IMsalSession
{
    private readonly IPublicClientApplication application;
    public MsalSession(SessionOptions options)
    {
        // No broker, shared-cache registration, Graph authrecord, serializer,
        // default client, device-code method or alternate flow is installed.
        application = PublicClientApplicationBuilder.Create(options.ClientId)
            .WithAuthority(AzureCloudInstance.AzurePublic, options.TenantId)
            .WithRedirectUri("http://localhost")
            .Build();
    }
    public string ClientId => application.AppConfig.ClientId;
    public Task<AuthenticationResult> AuthenticateAsync(string[] scopes, string account, CancellationToken cancellation) =>
        // MSAL defaults to SelectAccount, which overrides the expected account's
        // browser SSO experience. Keep the hint and let the identity service use
        // its existing session; the returned identity is still checked exactly.
        application.AcquireTokenInteractive(scopes).WithLoginHint(account).WithPrompt(Prompt.NoPrompt).WithUseEmbeddedWebView(false).ExecuteAsync(cancellation);
    public Task<AuthenticationResult> RefreshAsync(string[] scopes, IAccount account, CancellationToken cancellation) =>
        application.AcquireTokenSilent(scopes, account).WithForceRefresh(true).ExecuteAsync(cancellation);
}

public sealed class OwnedGraphSession : IDisposable
{
    private readonly SessionOptions options;
    private readonly IMsalSession credential;
    private readonly HttpClient http;
    private readonly CancellationTokenSource lifetime = new();
    private IAccount? account;
    private string? principal;
    private string[]? approvedGrants;
    private GraphObservation? observation;
    private bool disposed;

    internal OwnedGraphSession(SessionOptions options) : this(options, new MsalSession(options), new HttpClient(new SocketsHttpHandler
    {
        AllowAutoRedirect = false, UseCookies = false, ConnectTimeout = TimeSpan.FromSeconds(10)
    })) { }

    internal OwnedGraphSession(SessionOptions options, IMsalSession credential, HttpClient http)
    {
        this.options = options;
        this.credential = credential;
        this.http = http;
        ValidateOptions();
    }

    private void ValidateOptions()
    {
        if (!Guid.TryParse(options.ClientId, out var client) || client == Guid.Empty ||
            !Guid.TryParse(options.TenantId, out var tenant) || tenant == Guid.Empty ||
            options.Resource != "https://graph.microsoft.com" || options.Scopes.Length is < 1 or > 64 ||
            options.Account.Length is < 3 or > 320 || !options.Account.Contains('@') || options.Account.Any(char.IsWhiteSpace) ||
            options.Scopes.Any(scope => scope.Length is < 1 or > 512 || scope == ".default" || !scope.All(c => char.IsAsciiLetterOrDigit(c) || "._-".Contains(c))))
            throw new InvalidOperationException("Invalid exact Graph identity/scope binding.");
        bool Has(string scope) => options.Scopes.Contains(scope, StringComparer.OrdinalIgnoreCase);
        var allowed = options.ProbeUri switch
        {
            "https://graph.microsoft.com/v1.0/organization?$select=id" => Has("Directory.Read.All") || Has("Directory.ReadWrite.All"),
            "https://graph.microsoft.com/v1.0/identity/conditionalAccess/policies?$top=1&$select=id" => Has("Policy.Read.All") || Has("Policy.ReadWrite.ConditionalAccess"),
            "https://graph.microsoft.com/v1.0/me?$select=id" => Has("User.Read"),
            _ => false
        };
        if (!allowed) throw new InvalidOperationException("No registered read covered by the requested scopes.");
    }

    private string[] RequestedScopes => options.Scopes.Select(scope => options.Resource + "/" + scope).ToArray();

    private string[] Validate(AuthenticationResult result)
    {
        if (disposed || !string.Equals(credential.ClientId, options.ClientId, StringComparison.OrdinalIgnoreCase) ||
            result.Account is null || !string.Equals(result.Account.Username, options.Account, StringComparison.OrdinalIgnoreCase) ||
            !(string.Equals(result.Account.Environment, "login.microsoftonline.com", StringComparison.OrdinalIgnoreCase) || string.Equals(result.Account.Environment, "login.windows.net", StringComparison.OrdinalIgnoreCase)) ||
            !string.Equals(result.TenantId, options.TenantId, StringComparison.OrdinalIgnoreCase) ||
            string.IsNullOrEmpty(result.Account.HomeAccountId?.Identifier) ||
            string.IsNullOrEmpty(result.UniqueId) ||
            result.ExpiresOn <= DateTimeOffset.UtcNow.AddSeconds(30) || string.IsNullOrEmpty(result.AccessToken) ||
            !string.Equals(result.TokenType, "Bearer", StringComparison.OrdinalIgnoreCase))
            throw new InvalidOperationException("Graph credential identity/client/expiry mismatch.");
        var grants = result.Scopes.Select(scope => scope.StartsWith(options.Resource + "/", StringComparison.OrdinalIgnoreCase)
            ? scope[(options.Resource.Length + 1)..] : scope).Distinct(StringComparer.OrdinalIgnoreCase).Order(StringComparer.OrdinalIgnoreCase).ToArray();
        if (grants.Any(scope => scope.Contains('/') || scope.Contains(':')) || options.Scopes.Any(scope => !grants.Contains(scope, StringComparer.OrdinalIgnoreCase)))
            throw new InvalidOperationException("Actual granted delegated scopes do not cover the request.");
        if (principal is not null && (principal != result.Account.HomeAccountId!.Identifier + "|" + result.UniqueId || approvedGrants is null || !approvedGrants.SequenceEqual(grants, StringComparer.OrdinalIgnoreCase)))
            throw new InvalidOperationException("Graph principal or granted scopes changed after approval.");
        return grants;
    }

    private void Guard(Action operation)
    {
        try { operation(); }
        catch { Dispose(); throw; }
    }

    public void Authenticate() => Guard(() =>
    {
        if (disposed || account is not null) throw new InvalidOperationException("One explicit authentication per owned session.");
        var result = credential.AuthenticateAsync(RequestedScopes, options.Account, lifetime.Token).GetAwaiter().GetResult();
        approvedGrants = Validate(result);
        account = result.Account;
        principal = result.Account.HomeAccountId.Identifier + "|" + result.UniqueId;
        observation = new(options.Account, options.TenantId, options.ClientId, options.Resource, approvedGrants);
    });

    public void Probe() => Guard(() =>
    {
        if (disposed || account is null) throw new InvalidOperationException("No owned authenticated credential.");
        // Silent refresh never falls back to interaction. Validate the actual
        // result (including granted scopes), then send THIS result's token in
        // THIS process. No SDK context/client is reconstructed from shared state.
        var result = credential.RefreshAsync(RequestedScopes, account, lifetime.Token).GetAwaiter().GetResult();
        var grants = Validate(result);
        using var request = new HttpRequestMessage(HttpMethod.Get, options.ProbeUri);
        request.Headers.Authorization = new AuthenticationHeaderValue("Bearer", result.AccessToken);
        request.Headers.CacheControl = new CacheControlHeaderValue { NoCache = true };
        using var response = http.SendAsync(request, HttpCompletionOption.ResponseHeadersRead, lifetime.Token).GetAwaiter().GetResult();
        response.EnsureSuccessStatusCode();
        Validate(result);
        observation = new(options.Account, options.TenantId, options.ClientId, options.Resource, grants);
    });

    // Whitelist-only metadata is the only managed output available to PowerShell.
    public string EvidenceJson() => JsonSerializer.Serialize(observation ?? throw new InvalidOperationException("No evidence."), Bootstrap.JsonOptions);

    public void Dispose()
    {
        if (disposed) return;
        disposed = true;
        lifetime.Cancel(); http.Dispose(); lifetime.Dispose();
        account = null; principal = null; approvedGrants = null; observation = null;
        // The owning process exits next, disposing its solely in-memory SDK cache.
    }
}
