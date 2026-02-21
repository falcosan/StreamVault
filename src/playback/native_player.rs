use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSBackingStoreType, NSPanel, NSWindowOrderingMode, NSWindowStyleMask,
};
use objc2_av_foundation::AVPlayer;
use objc2_av_kit::AVPlayerView;
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_foundation::{NSString, NSURL};

pub struct NativeVideoPlayer {
    player: Retained<AVPlayer>,
    panel: Retained<NSPanel>,
    _player_view: Retained<AVPlayerView>,
    mtm: MainThreadMarker,
    playing: bool,
}

impl NativeVideoPlayer {
    pub fn play(url: &str, title: &str, mtm: MainThreadMarker) -> Result<Self, String> {
        let ns_url_string: Retained<NSString> = NSString::from_str(url);
        let ns_url =
            NSURL::URLWithString(&ns_url_string).ok_or_else(|| format!("Invalid URL: {url}"))?;

        let player = unsafe { AVPlayer::initWithURL(AVPlayer::alloc(mtm), &ns_url) };

        let frame = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(960.0, 540.0));
        let player_view = unsafe { AVPlayerView::initWithFrame(AVPlayerView::alloc(mtm), frame) };
        unsafe { player_view.setPlayer(Some(&player)) };

        let style = NSWindowStyleMask::Titled
            | NSWindowStyleMask::Closable
            | NSWindowStyleMask::Resizable
            | NSWindowStyleMask::Miniaturizable;

        let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
            NSPanel::alloc(mtm),
            frame,
            style,
            NSBackingStoreType::Buffered,
            false,
        );

        panel.setContentView(Some(&player_view));
        let ns_title: Retained<NSString> = NSString::from_str(title);
        panel.setTitle(&ns_title);

        let app = NSApplication::sharedApplication(mtm);
        if let Some(main_window) = app.mainWindow() {
            unsafe {
                main_window.addChildWindow_ordered(&panel, NSWindowOrderingMode::Above);
            }

            let main_frame = main_window.frame();
            let panel_x = main_frame.origin.x + (main_frame.size.width - 960.0) / 2.0;
            let panel_y = main_frame.origin.y + (main_frame.size.height - 540.0) / 2.0;
            let panel_frame =
                CGRect::new(CGPoint::new(panel_x, panel_y), CGSize::new(960.0, 540.0));
            panel.setFrame_display(panel_frame, true);
        }

        panel.makeKeyAndOrderFront(None);
        unsafe { player.play() };

        Ok(Self {
            player,
            panel,
            _player_view: player_view,
            mtm,
            playing: true,
        })
    }

    pub fn pause(&mut self) {
        unsafe { self.player.pause() };
        self.playing = false;
    }

    pub fn resume(&mut self) {
        unsafe { self.player.play() };
        self.playing = true;
    }

    pub fn stop(&mut self) {
        unsafe { self.player.pause() };
        self.playing = false;

        let app = NSApplication::sharedApplication(self.mtm);
        if let Some(main_window) = app.mainWindow() {
            main_window.removeChildWindow(&self.panel);
        }
        self.panel.orderOut(None);
        self.panel.close();
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }
}
