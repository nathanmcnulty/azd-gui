using System.Net.Http.Headers;
using System.Reflection;
using System.Runtime.CompilerServices;
using System.Runtime.Loader;
using System.Text.Json;
using System.Security.Cryptography;
using System.Text;
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

public sealed record SessionOptions(string RequirementsDigest, string Account, string TenantId, string ClientId, string Resource, string[] Scopes, string ProbeUri, string? Purpose = null);
public sealed record GraphObservation(string Account, string TenantId, string ClientId, string Resource, string[] Scopes, string AuthType = "Delegated", string ContextScope = "Process", string Environment = "Global", string? PrincipalFingerprint = null, int VerificationReads = 0, int PimReadsAttempted = 0, int PimReadsCompleted = 0);

public sealed class GraphBoundaryException(string code) : InvalidOperationException(code)
{
    public string Code { get; } = code;
}

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
    private readonly Dictionary<string, string> pimContinuations = new(StringComparer.Ordinal);
    private int pimReads;
    private int pimBytes;
    private int verificationReads;
    private int pimReadsAttempted;
    private int pimReadsCompleted;
    private string? auditSession;
    private long auditSequence;

    public void BeginPimPlanAudit(string sessionId, long sequence)
    {
        if (options.Purpose != "pimPlan" || disposed || account is null || !Guid.TryParse(sessionId, out _) || sequence < 1)
            throw new GraphBoundaryException("InvalidPimSession");
        auditSession = sessionId;
        auditSequence = sequence;
    }

    private GraphObservation Observe(string[] grants) => new(options.Account, options.TenantId, options.ClientId, options.Resource, grants,
        PrincipalFingerprint: principal is null ? null : Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(principal))).ToLowerInvariant(),
        VerificationReads: verificationReads, PimReadsAttempted: pimReadsAttempted, PimReadsCompleted: pimReadsCompleted);

    private void Progress(string kind, string[] grants)
    {
        observation = Observe(grants);
        if (auditSession is null) return;
        Console.Out.WriteLine("AZD_GRAPH_PROGRESS:" + JsonSerializer.Serialize(new {
            sequence = auditSequence, sessionId = auditSession, requirementsDigest = options.RequirementsDigest,
            processId = Environment.ProcessId, kind, evidence = observation
        }, Bootstrap.JsonOptions));
        Console.Out.Flush();
    }

    // The planner receives this reader, never the credential or an access token.
    public string TenantId => options.TenantId;

    private static readonly Dictionary<string, string> PimReads = new(StringComparer.Ordinal)
    {
        ["https://graph.microsoft.com/beta/roleManagement/directory/roleDefinitions?$filter=isPrivileged%20eq%20true"] = "RoleManagement.Read.Directory",
        ["https://graph.microsoft.com/beta/roleManagement/directory/roleDefinitions?$filter=isPrivileged%20eq%20false"] = "RoleManagement.Read.Directory",
        ["https://graph.microsoft.com/v1.0/policies/roleManagementPolicyAssignments?$filter=scopeId%20eq%20%27%2F%27%20and%20scopeType%20eq%20%27DirectoryRole%27&$top=999"] = "RoleManagement.Read.Directory",
        ["https://graph.microsoft.com/v1.0/policies/roleManagementPolicies?$filter=scopeId%20eq%20%27%2F%27%20and%20scopeType%20eq%20%27DirectoryRole%27&$expand=rules&$top=999"] = "RoleManagement.Read.Directory",
        ["https://graph.microsoft.com/v1.0/identity/conditionalAccess/authenticationContextClassReferences"] = "AuthenticationContext.Read.All",
        ["https://graph.microsoft.com/beta/identity/conditionalAccess/policies?$top=999"] = "Policy.Read.All",
        ["https://graph.microsoft.com/v1.0/policies/authenticationStrengthPolicies?$top=100"] = "Policy.Read.All"
    };

    private static Dictionary<string, string> Query(Uri uri)
    {
        var query = new Dictionary<string, string>(StringComparer.Ordinal);
        foreach (var part in uri.Query.TrimStart('?').Split('&', StringSplitOptions.RemoveEmptyEntries))
        {
            var pair = part.Split('=', 2);
            if (pair.Length != 2 || !query.TryAdd(Uri.UnescapeDataString(pair[0]), Uri.UnescapeDataString(pair[1].Replace("+", " "))))
                throw new GraphBoundaryException("InvalidPimResponse");
        }
        return query;
    }

    private static bool SameCollectionQuery(Uri current, Uri next)
    {
        var original = Query(current);
        var continuation = Query(next);
        static bool Paging(string key) => key is "$skiptoken" or "$skip";
        return continuation.Keys.Any(Paging) &&
            original.Where(p => !Paging(p.Key)).All(p => continuation.TryGetValue(p.Key, out var value) && value == p.Value) &&
            continuation.All(p => Paging(p.Key) || original.TryGetValue(p.Key, out var value) && value == p.Value);
    }

    public string ReadPimJson(string uri)
    {
        string? body = null;
        Guard(() =>
        {
            if (options.Purpose != "pimPlan" || disposed || account is null || verificationReads != 1 || ++pimReads > 100)
                throw new GraphBoundaryException("PimReadBudgetOrSessionRejected");
            string? requiredScope;
            if (!PimReads.TryGetValue(uri, out requiredScope) && !pimContinuations.Remove(uri, out requiredScope))
                throw new GraphBoundaryException("UnregisteredPimRead");
            if (!options.Scopes.Contains(requiredScope, StringComparer.OrdinalIgnoreCase))
                throw new GraphBoundaryException("GraphDelegatedScopeMissing");
            using var timeout = CancellationTokenSource.CreateLinkedTokenSource(lifetime.Token);
            timeout.CancelAfter(TimeSpan.FromSeconds(30));
            var result = credential.RefreshAsync(RequestedScopes, account, timeout.Token).GetAwaiter().GetResult();
            var grants = Validate(result);
            using var request = new HttpRequestMessage(HttpMethod.Get, uri);
            request.Headers.Authorization = new AuthenticationHeaderValue("Bearer", result.AccessToken);
            request.Headers.CacheControl = new CacheControlHeaderValue { NoCache = true };
            pimReadsAttempted++;
            Progress("readStarted", grants);
            using var response = http.SendAsync(request, HttpCompletionOption.ResponseHeadersRead, timeout.Token).GetAwaiter().GetResult();
            RequireReadSuccess(response);
            using var stream = response.Content.ReadAsStreamAsync(timeout.Token).GetAwaiter().GetResult();
            using var bytes = new MemoryStream();
            var buffer = new byte[8192];
            int count;
            while ((count = stream.ReadAsync(buffer, timeout.Token).AsTask().GetAwaiter().GetResult()) != 0)
            {
                pimBytes += count;
                if (bytes.Length + count > 4 * 1024 * 1024 || pimBytes > 16 * 1024 * 1024)
                    throw new GraphBoundaryException("PimResponseLimitExceeded");
                bytes.Write(buffer, 0, count);
            }
            using var json = JsonDocument.Parse(bytes.ToArray());
            if (json.RootElement.ValueKind != JsonValueKind.Object ||
                !json.RootElement.TryGetProperty("value", out var values) || values.ValueKind != JsonValueKind.Array)
                throw new GraphBoundaryException("InvalidPimResponse");
            if (json.RootElement.TryGetProperty("@odata.nextLink", out var next) && next.ValueKind != JsonValueKind.Null)
            {
                var link = next.GetString() ?? throw new GraphBoundaryException("InvalidPimResponse");
                var current = new Uri(uri);
                if (link.Length > 8192 || !Uri.TryCreate(link, UriKind.Absolute, out var target) ||
                    target.Scheme != "https" || target.Host != "graph.microsoft.com" || target.Port != 443 ||
                    target.UserInfo.Length != 0 || target.Fragment.Length != 0 || target.AbsolutePath != current.AbsolutePath ||
                    link == uri || pimContinuations.ContainsKey(link) || !SameCollectionQuery(current, target))
                    throw new GraphBoundaryException("InvalidPimResponse");
                // Only the exact continuation returned by this protected response
                // may be used, for the same collection and permission.
                pimContinuations.Add(link, requiredScope);
            }
            Validate(result);
            pimReadsCompleted++;
            Progress("readCompleted", grants);
            body = json.RootElement.GetRawText();
        });
        return body!;
    }

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
        if (options.Purpose is not null && options.Purpose != "pimPlan") throw new GraphBoundaryException("InvalidPimSession");
        if (options.Purpose == "pimPlan" && (options.ProbeUri != "https://graph.microsoft.com/v1.0/identity/conditionalAccess/policies?$top=1&$select=id" ||
            options.Scopes.Any(s => s is not ("AuthenticationContext.Read.All" or "Policy.Read.All" or "RoleManagement.Read.Directory"))))
            throw new GraphBoundaryException("InvalidPimSession");
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

    private static void RequireReadSuccess(HttpResponseMessage response)
    {
        if (response.IsSuccessStatusCode) return;
        throw new GraphBoundaryException((int)response.StatusCode switch {
            401 or 403 => "GraphReadDenied", >= 300 and < 400 => "GraphRedirectRejected", _ => "GraphReadFailed"
        });
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
            throw new GraphBoundaryException("GraphIdentityMismatch");
        var grants = result.Scopes.Select(scope => scope.StartsWith(options.Resource + "/", StringComparison.OrdinalIgnoreCase)
            ? scope[(options.Resource.Length + 1)..] : scope).Distinct(StringComparer.OrdinalIgnoreCase).Order(StringComparer.OrdinalIgnoreCase).ToArray();
        if (grants.Any(scope => scope.Contains('/') || scope.Contains(':')) || options.Scopes.Any(scope => !grants.Contains(scope, StringComparer.OrdinalIgnoreCase)))
            throw new GraphBoundaryException("GraphDelegatedScopeMissing");
        if (principal is not null && (principal != result.Account.HomeAccountId!.Identifier + "|" + result.UniqueId || approvedGrants is null || !approvedGrants.SequenceEqual(grants, StringComparer.OrdinalIgnoreCase)))
            throw new GraphBoundaryException("GraphIdentityOrGrantsChanged");
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
        observation = Observe(approvedGrants);
    });

    public void Probe() => Guard(() =>
    {
        if (disposed || account is null) throw new InvalidOperationException("No owned authenticated credential.");
        if (options.Purpose == "pimPlan" && verificationReads != 0) throw new GraphBoundaryException("PimVerificationBudgetExceeded");
        // Silent refresh never falls back to interaction. Validate the actual
        // result (including granted scopes), then send THIS result's token in
        // THIS process. No SDK context/client is reconstructed from shared state.
        var result = credential.RefreshAsync(RequestedScopes, account, lifetime.Token).GetAwaiter().GetResult();
        var grants = Validate(result);
        using var request = new HttpRequestMessage(HttpMethod.Get, options.ProbeUri);
        request.Headers.Authorization = new AuthenticationHeaderValue("Bearer", result.AccessToken);
        request.Headers.CacheControl = new CacheControlHeaderValue { NoCache = true };
        verificationReads++;
        Progress("verificationStarted", grants);
        using var response = http.SendAsync(request, HttpCompletionOption.ResponseHeadersRead, lifetime.Token).GetAwaiter().GetResult();
        RequireReadSuccess(response);
        Validate(result);
        Progress("verificationCompleted", grants);
    });

    // Authentication evidence contains whitelist-only metadata. PIM collection
    // JSON stays inside the worker until transformed by the pinned planner.
    public string EvidenceJson() => JsonSerializer.Serialize(observation ?? throw new InvalidOperationException("No evidence."), Bootstrap.JsonOptions);

    public void Dispose()
    {
        if (disposed) return;
        disposed = true;
        lifetime.Cancel(); http.Dispose(); lifetime.Dispose();
        account = null; principal = null; approvedGrants = null; observation = null;
        pimContinuations.Clear();
        // The owning process exits next, disposing its solely in-memory SDK cache.
    }
}
