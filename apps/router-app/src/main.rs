use fncc::*;
use std::sync::Mutex;

include!(concat!(env!("OUT_DIR"), "/generated.rs"));

static ROUTER: Mutex<Option<Router<Route>>> = Mutex::new(None);

fn with_router<F, R>(f: F) -> R
where
    F: FnOnce(&mut Router<Route>) -> R,
{
    let mut guard = ROUTER.lock().unwrap();
    let router = guard.get_or_insert_with(|| Router::new(Route::Index));
    f(router)
}

#[fncc::command]
fn nav_home() {
    with_router(|r| r.push(Route::Index));
}

#[fncc::command]
fn nav_settings() {
    with_router(|r| r.push(Route::Settings));
}

#[fncc::command]
fn nav_analytics() {
    with_router(|r| r.push(Route::Analytics));
}

#[fncc::command]
fn nav_user_alice() {
    with_router(|r| {
        r.push(Route::UsersId {
            id: "alice".to_string(),
        })
    });
}

#[fncc::command]
fn nav_back() {
    with_router(|r| {
        r.pop();
    });
}

#[derive(Default)]
struct AppState;

impl Render for AppState {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let route = with_router(|r| r.current().clone());
        render_router_outlet(&route)
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(700.), px(500.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|_| AppState),
        )
        .unwrap();
    });
}
