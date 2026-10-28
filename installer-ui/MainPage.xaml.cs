using System.Diagnostics;
using System.Text;
using Microsoft.UI;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media.Animation;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Shapes;
using Windows.Storage.Pickers;
using WinRT.Interop;
using SystemPath = System.IO.Path;

namespace SelorynInstaller;

public sealed partial class MainPage : Page
{
    private const int WelcomePhase = 1;
    private const int LocationPhase = 2;
    private const int InstallPhase = 3;
    private const int DonePhase = 4;

    private readonly Brush _mintBrush;
    private readonly Brush _mintBrightBrush;
    private readonly Brush _surfaceBrush;
    private readonly Brush _borderBrush;
    private readonly Brush _textBrush;
    private readonly Brush _mutedBrush;
    private readonly Brush _dimBrush;
    private Process? _installProcess;
    private Storyboard? _ambientStoryboard;
    private string _installRoot = string.Empty;
    private int _phase = WelcomePhase;
    private bool _installSucceeded;

    public MainPage()
    {
        InitializeComponent();

        _mintBrush = (Brush)Application.Current.Resources["MintBrush"];
        _mintBrightBrush = (Brush)Application.Current.Resources["MintBrightBrush"];
        _surfaceBrush = (Brush)Application.Current.Resources["SurfaceBrush"];
        _borderBrush = (Brush)Application.Current.Resources["BorderBrush"];
        _textBrush = (Brush)Application.Current.Resources["TextBrush"];
        _mutedBrush = (Brush)Application.Current.Resources["TextMutedBrush"];
        _dimBrush = (Brush)Application.Current.Resources["DimBrush"];

        PathTextBox.Text = DefaultInstallRoot();
        Loaded += Page_Loaded;
    }

    private static string DefaultInstallRoot() => SystemPath.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        "Seloryn");

    private void Page_Loaded(object sender, RoutedEventArgs e)
    {
        SetPhase(WelcomePhase);
        StartBrandMotion();
    }

    private void StartBrandMotion()
    {
        _ambientStoryboard?.Stop();
        _ambientStoryboard = new Storyboard();
        AddLoopAnimation(_ambientStoryboard, AmbientFlowTop, "StrokeDashOffset", 50, 0, 3200);
        AddLoopAnimation(_ambientStoryboard, AmbientFlowBottom, "StrokeDashOffset", 40, 0, 4700, 250);
        AddLoopAnimation(_ambientStoryboard, AmbientFlowAccent, "StrokeDashOffset", 30, 0, 2800, 500);
        AddLoopAnimation(_ambientStoryboard, AmbientFlowTop, "Opacity", 0.48, 0.82, 3600, 100);
        _ambientStoryboard.Begin();

        var logoStoryboard = new Storyboard();
        AddDrawAnimation(logoStoryboard, BrandLogo, "Opacity", 0, 1, 650, 200);
        AddDrawAnimation(logoStoryboard, BrandLogoTransform, "TranslateX", -12, 0, 850, 200);
        AddDrawAnimation(logoStoryboard, BrandLogoTransform, "ScaleX", 0.97, 1, 850, 200);
        AddDrawAnimation(logoStoryboard, BrandLogoTransform, "ScaleY", 0.97, 1, 850, 200);
        logoStoryboard.Begin();
    }

    private static void AddDrawAnimation(Storyboard storyboard, DependencyObject target, string property, double from, double to, int milliseconds, int delay = 0)
    {
        var animation = new DoubleAnimation
        {
            From = from,
            To = to,
            Duration = new Duration(TimeSpan.FromMilliseconds(milliseconds)),
            BeginTime = TimeSpan.FromMilliseconds(delay),
            EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut },
        };
        Storyboard.SetTarget(animation, target);
        Storyboard.SetTargetProperty(animation, property);
        storyboard.Children.Add(animation);
    }

    private static void AddLoopAnimation(Storyboard storyboard, DependencyObject target, string property, double from, double to, int milliseconds, int delay = 0)
    {
        var animation = new DoubleAnimation
        {
            From = from,
            To = to,
            Duration = new Duration(TimeSpan.FromMilliseconds(milliseconds)),
            BeginTime = TimeSpan.FromMilliseconds(delay),
            AutoReverse = true,
            RepeatBehavior = RepeatBehavior.Forever,
            EasingFunction = new CubicEase { EasingMode = EasingMode.EaseInOut },
        };
        Storyboard.SetTarget(animation, target);
        Storyboard.SetTargetProperty(animation, property);
        storyboard.Children.Add(animation);
    }

    private void SetPhase(int phase)
    {
        _phase = phase;
        WelcomePanel.Visibility = phase == WelcomePhase ? Visibility.Visible : Visibility.Collapsed;
        LocationPanel.Visibility = phase == LocationPhase ? Visibility.Visible : Visibility.Collapsed;
        InstallPanel.Visibility = phase == InstallPhase ? Visibility.Visible : Visibility.Collapsed;
        DonePanel.Visibility = phase == DonePhase ? Visibility.Visible : Visibility.Collapsed;

        BackButton.Visibility = phase == LocationPhase ? Visibility.Visible : Visibility.Collapsed;
        BackButton.IsEnabled = phase == LocationPhase;
        BrowseButton.IsEnabled = phase == LocationPhase;
        PathTextBox.IsEnabled = phase == LocationPhase;
        AddPathCheckBox.IsEnabled = phase == LocationPhase;
        MakeDefaultCheckBox.IsEnabled = phase == LocationPhase;
        CleanPreviousCheckBox.IsEnabled = phase == LocationPhase;

        switch (phase)
        {
            case WelcomePhase:
                StatusText.Text = "Ready to begin.";
                PrimaryButton.Content = "Continue  ›";
                PrimaryButton.IsEnabled = true;
                CancelButton.Content = "Cancel";
                CancelButton.IsEnabled = true;
                break;
            case LocationPhase:
                StatusText.Text = "Choose an install location.";
                PrimaryButton.Content = "Install  ›";
                PrimaryButton.IsEnabled = true;
                CancelButton.Content = "Cancel";
                CancelButton.IsEnabled = true;
                break;
            case InstallPhase:
                StatusText.Text = "Installing…";
                PrimaryButton.Content = "Installing…";
                PrimaryButton.IsEnabled = false;
                CancelButton.Content = "Cancel";
                CancelButton.IsEnabled = false;
                break;
            case DonePhase:
                StatusText.Text = "Installation complete.";
                PrimaryButton.Content = "Open terminal  ↗";
                PrimaryButton.IsEnabled = true;
                CancelButton.Content = "Close";
                CancelButton.IsEnabled = true;
                break;
        }

        UpdateStepper(phase);
    }

    private void UpdateStepper(int phase)
    {
        SetStep(Step1Dot, Step1Number, "01", phase >= WelcomePhase, phase > WelcomePhase);
        SetStep(Step2Dot, Step2Number, "02", phase >= LocationPhase, phase > LocationPhase);
        SetStep(Step3Dot, Step3Number, "03", phase >= InstallPhase, phase > InstallPhase);
        SetStep(Step4Dot, Step4Number, "04", phase >= DonePhase, false);
        StepLine1.Fill = phase > WelcomePhase ? _mintBrush : _borderBrush;
        StepLine2.Fill = phase > LocationPhase ? _mintBrush : _borderBrush;
        StepLine3.Fill = phase > InstallPhase ? _mintBrush : _borderBrush;
    }

    private void SetStep(Ellipse dot, TextBlock number, string ordinal, bool active, bool completed)
    {
        dot.Fill = active ? _mintBrush : _surfaceBrush;
        dot.Stroke = active ? _mintBrightBrush : _borderBrush;
        number.Foreground = active ? new SolidColorBrush(Colors.Black) : _mutedBrush;
        number.Text = completed ? "✓" : ordinal;
    }

    private void PrimaryButton_Click(object sender, RoutedEventArgs e)
    {
        if (_phase == WelcomePhase)
        {
            SetPhase(LocationPhase);
            return;
        }

        if (_phase == LocationPhase)
        {
            _ = StartInstallAsync();
            return;
        }

        if (_phase == InstallPhase && !_installSucceeded)
        {
            _ = StartInstallAsync();
            return;
        }

        if (_phase == DonePhase)
        {
            OpenTerminalAndClose();
        }
    }

    private void BackButton_Click(object sender, RoutedEventArgs e)
    {
        if (_phase == LocationPhase)
        {
            SetPhase(WelcomePhase);
        }
    }

    private void CancelButton_Click(object sender, RoutedEventArgs e)
    {
        if (_installProcess is not null)
        {
            return;
        }

        App.MainWindow?.Close();
    }

    private async void BrowseButton_Click(object sender, RoutedEventArgs e)
    {
        var picker = new FolderPicker();
        picker.FileTypeFilter.Add("*");
        var hwnd = WindowNative.GetWindowHandle(App.MainWindow);
        InitializeWithWindow.Initialize(picker, hwnd);
        var folder = await picker.PickSingleFolderAsync();
        if (folder is not null)
        {
            PathTextBox.Text = folder.Path;
        }
    }

    private async Task StartInstallAsync()
    {
        if (!TryGetInstallRoot(out var root, out var error))
        {
            StatusText.Text = error;
            return;
        }

        _installRoot = root;
        _installSucceeded = false;
        LogText.Text = string.Empty;
        InstallProgress.Value = 0;
        ProgressText.Text = "0%";
        InstallStatus.Text = "Preparing the installation…";
        SetPhase(InstallPhase);

        var backendPath = SystemPath.Combine(AppContext.BaseDirectory, "Assets", "seloryn-setup-backend.exe");
        if (!File.Exists(backendPath))
        {
            await FinishInstallAsync(false, "The installer backend is missing from this package.");
            return;
        }

        var startInfo = new ProcessStartInfo(backendPath)
        {
            WorkingDirectory = AppContext.BaseDirectory,
            UseShellExecute = false,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            CreateNoWindow = true,
            StandardOutputEncoding = Encoding.UTF8,
            StandardErrorEncoding = Encoding.UTF8,
        };
        startInfo.ArgumentList.Add("--headless-progress");
        startInfo.ArgumentList.Add("--install-root");
        startInfo.ArgumentList.Add(root);
        if (AddPathCheckBox.IsChecked == true) startInfo.ArgumentList.Add("--add-path");
        if (MakeDefaultCheckBox.IsChecked == true) startInfo.ArgumentList.Add("--make-default");
        if (CleanPreviousCheckBox.IsChecked == true) startInfo.ArgumentList.Add("--clean-previous");

        try
        {
            _installProcess = new Process { StartInfo = startInfo, EnableRaisingEvents = true };
            if (!_installProcess.Start())
            {
                await FinishInstallAsync(false, "Could not start the installer backend.");
                return;
            }

            var completeOk = false;
            var completeMessage = string.Empty;
            while (await _installProcess.StandardOutput.ReadLineAsync() is { } line)
            {
                if (line.StartsWith("PROGRESS\t", StringComparison.Ordinal))
                {
                    var parts = line.Split('\t', 3);
                    if (parts.Length >= 3 && int.TryParse(parts[1], out var progress))
                    {
                        InstallProgress.Value = Math.Clamp(progress, 0, 100);
                        ProgressText.Text = $"{Math.Clamp(progress, 0, 100)}%";
                        InstallStatus.Text = parts[2];
                        AppendLog(parts[2]);
                    }
                    continue;
                }

                if (line.StartsWith("COMPLETE\t", StringComparison.Ordinal))
                {
                    var parts = line.Split('\t', 3);
                    completeOk = parts.Length >= 2 && parts[1] == "OK";
                    completeMessage = parts.Length >= 3 ? parts[2] : string.Empty;
                    if (!string.IsNullOrWhiteSpace(completeMessage)) AppendLog(completeMessage);
                    continue;
                }

                if (!string.IsNullOrWhiteSpace(line)) AppendLog(line);
            }

            var standardError = await _installProcess.StandardError.ReadToEndAsync();
            await _installProcess.WaitForExitAsync();
            var exitCode = _installProcess.ExitCode;
            var success = completeOk && exitCode == 0;
            var resultMessage = success
                ? (string.IsNullOrWhiteSpace(completeMessage) ? "Installation complete." : completeMessage)
                : (string.IsNullOrWhiteSpace(completeMessage) ? standardError.Trim() : completeMessage);

            if (!success && string.IsNullOrWhiteSpace(resultMessage))
            {
                resultMessage = $"The installer backend exited with code {exitCode}.";
            }

            await FinishInstallAsync(success, resultMessage);
        }
        catch (Exception exception)
        {
            await FinishInstallAsync(false, exception.Message);
        }
        finally
        {
            _installProcess?.Dispose();
            _installProcess = null;
        }
    }

    private async Task FinishInstallAsync(bool success, string message)
    {
        _installSucceeded = success;
        AppendLog(message);

        if (success)
        {
            InstallProgress.Value = 100;
            ProgressText.Text = "100%";
            InstallStatus.Text = "Installation complete.";
            DonePathText.Text = _installRoot;
            SetPhase(DonePhase);
            return;
        }

        InstallStatus.Text = "Installation could not be completed.";
        StatusText.Text = message;
        PrimaryButton.Content = "Retry  ↻";
        PrimaryButton.IsEnabled = true;
        CancelButton.Content = "Close";
        CancelButton.IsEnabled = true;
        await Task.CompletedTask;
    }

    private void AppendLog(string message)
    {
        if (string.IsNullOrWhiteSpace(message)) return;
        LogText.Text += (LogText.Text.Length == 0 ? string.Empty : Environment.NewLine) + message;
        LogScrollViewer.ChangeView(null, double.MaxValue, null);
    }

    private bool TryGetInstallRoot(out string root, out string error)
    {
        root = string.Empty;
        error = string.Empty;
        var raw = PathTextBox.Text.Trim();
        if (string.IsNullOrWhiteSpace(raw))
        {
            error = "Choose an install folder first.";
            return false;
        }

        try
        {
            root = SystemPath.GetFullPath(raw);
            if (!SystemPath.IsPathRooted(root)) throw new ArgumentException();
            return true;
        }
        catch
        {
            error = "That install path is not valid.";
            return false;
        }
    }

    private void OpenTerminalAndClose()
    {
        try
        {
            var terminal = new ProcessStartInfo("cmd.exe")
            {
                UseShellExecute = false,
                WorkingDirectory = _installRoot,
            };
            terminal.ArgumentList.Add("/K");
            terminal.ArgumentList.Add("seloryn --version");
            var path = Environment.GetEnvironmentVariable("PATH") ?? string.Empty;
            terminal.Environment["PATH"] = SystemPath.Combine(_installRoot, "bin") + ";" + path;
            terminal.Environment["SELORYN_HOME"] = _installRoot;
            Process.Start(terminal);
        }
        finally
        {
            App.MainWindow?.Close();
        }
    }
}
