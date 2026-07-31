use fncc::*;

include!(concat!(env!("OUT_DIR"), "/generated.rs"));

#[fncc::command]
fn nav_home(state: &mut AppState, cx: &mut Context<AppState>) {
    state.router.push(Route::Index);
    cx.notify();
}

#[fncc::command]
fn nav_settings(state: &mut AppState, cx: &mut Context<AppState>) {
    state.router.push(Route::Settings);
    cx.notify();
}

#[fncc::command]
fn nav_analytics(state: &mut AppState, cx: &mut Context<AppState>) {
    state.router.push(Route::Analytics);
    cx.notify();
}

#[fncc::command]
fn nav_user_alice(state: &mut AppState, cx: &mut Context<AppState>) {
    state.router.push(Route::UsersId {
        id: "alice".to_string(),
    });
    cx.notify();
}

#[fncc::command]
fn nav_back(state: &mut AppState, cx: &mut Context<AppState>) {
    state.router.pop();
    cx.notify();
}

struct AppState {
    router: Router<Route>,
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(700.), px(500.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|_| AppState {
                    router: Router::new(Route::Index),
                })
            },
        )
        .unwrap();
    });
}
