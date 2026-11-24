using System;
using System.Diagnostics;
using System.IO;
using System.Threading;
using System.Windows.Automation;

static class Smoke
{
    [STAThread]
    static int Main(string[] args)
    {
        try
        {
            if (!Environment.UserInteractive || Process.GetCurrentProcess().SessionId == 0)
                throw new InvalidOperationException("Windows UI Automation needs an interactive desktop in the runner session.");
            var desktop = AutomationElement.RootElement;
            if (desktop == null) throw new InvalidOperationException("The runner's UI Automation desktop is unavailable.");
            if (args is ["--check-desktop"])
            {
                Console.WriteLine("Interactive desktop preflight passed");
                return 0;
            }
            if (args is not ["--inspect", var pid, var title, var fixture] || !File.Exists(fixture))
                throw new InvalidOperationException("Expected an owned app process, exact title, and synthetic file.");

            using var app = Process.GetProcessById(int.Parse(pid));
            if (app.SessionId != Process.GetCurrentProcess().SessionId)
                throw new InvalidOperationException("The owned app and UI Automation must share a desktop session.");

            var window = Wait("the owned app window", () => desktop.FindFirst(TreeScope.Children, new AndCondition(
                new PropertyCondition(AutomationElement.ProcessIdProperty, app.Id),
                new PropertyCondition(AutomationElement.NameProperty, title))));
            if (window.Current.ProcessId != app.Id)
                throw new InvalidOperationException("The app window owner changed.");

            Button(window, "Unsigned provenance fixture", prefix: true).Invoke();
            ConfirmProvenance(window, "Verified");
            File.WriteAllText(fixture, "synthetic local component changed");
            var panel = Wait("the shared component panel", () => FindName(window, "Shared component provenance"));
            Button(panel, "Refresh components").Invoke();
            ConfirmProvenance(window, "Modified");
            return 0;
        }
        catch (Exception error)
        {
            Console.Error.WriteLine(error.GetType().Name + ": " + error.Message);
            return 1;
        }
    }

    static void ConfirmProvenance(AutomationElement window, string status)
    {
        Wait("synthetic component 1.0.0 " + status, () =>
        {
            var panel = FindName(window, "Shared component provenance");
            var component = panel == null ? null : FindName(panel, "Component synthetic-component");
            return component != null && FindName(component, "1.0.0") != null ? FindName(component, status) : null;
        });
        Console.WriteLine("UI showed synthetic-component 1.0.0 " + status);
    }

    static InvokePattern Button(AutomationElement scope, string label, bool prefix = false)
    {
        var button = Wait("the " + label + " button", () =>
        {
            foreach (AutomationElement candidate in scope.FindAll(TreeScope.Descendants,
                new PropertyCondition(AutomationElement.ControlTypeProperty, ControlType.Button)))
            {
                var name = candidate.Current.Name;
                if ((name == label || (prefix && name.StartsWith(label + " ", StringComparison.Ordinal))) &&
                    candidate.Current.IsEnabled && candidate.TryGetCurrentPattern(InvokePattern.Pattern, out var pattern))
                    return (InvokePattern)pattern;
            }
            return null;
        });
        return button;
    }

    static AutomationElement? FindName(AutomationElement scope, string name) =>
        scope.FindFirst(TreeScope.Descendants, new PropertyCondition(AutomationElement.NameProperty, name));

    static T Wait<T>(string stage, Func<T?> read) where T : class
    {
        var deadline = Stopwatch.StartNew();
        while (deadline.Elapsed < TimeSpan.FromSeconds(15))
        {
            try
            {
                var value = read();
                if (value != null) return value;
            }
            catch (ElementNotAvailableException) { }
            Thread.Sleep(200);
        }
        throw new TimeoutException("UI Automation timed out waiting for " + stage + ".");
    }
}
