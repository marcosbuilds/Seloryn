using Microsoft.UI;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Windows.Graphics;
using WinRT.Interop;

// To learn more about WinUI, the WinUI project structure,
// and more about our project templates, see: http://aka.ms/winui-project-info.

namespace SelorynInstaller;

/// <summary>
/// The application window. This hosts a Frame that displays pages. Add your
/// UI and logic to MainPage.xaml / MainPage.xaml.cs instead of here so you
/// can use Page features such as navigation events and the Loaded lifecycle.
/// </summary>
public sealed partial class MainWindow : Window
{
    public MainWindow()
    {
        InitializeComponent();

        ExtendsContentIntoTitleBar = true;
        SetTitleBar(AppTitleBar);

        AppWindow.Resize(new SizeInt32(860, 660));
        AppWindow.TitleBar.BackgroundColor = Colors.Black;
        AppWindow.TitleBar.ForegroundColor = Colors.White;
        AppWindow.TitleBar.ButtonBackgroundColor = Colors.Black;
        AppWindow.TitleBar.ButtonForegroundColor = Colors.White;
        AppWindow.TitleBar.ButtonHoverBackgroundColor = ColorHelper.FromArgb(255, 17, 35, 29);
        AppWindow.TitleBar.ButtonHoverForegroundColor = Colors.White;
        AppWindow.TitleBar.ButtonPressedBackgroundColor = ColorHelper.FromArgb(255, 43, 209, 160);
        AppWindow.TitleBar.ButtonPressedForegroundColor = Colors.Black;

        if (AppWindow.Presenter is OverlappedPresenter presenter)
        {
            presenter.IsResizable = false;
            presenter.IsMaximizable = false;
            presenter.IsMinimizable = true;
        }

        // Navigate the root frame to the main page on startup.
        RootFrame.Navigate(typeof(MainPage));
    }
}
