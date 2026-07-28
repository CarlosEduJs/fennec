use fennec::*;

include!(concat!(env!("OUT_DIR"), "/generated.rs"));

struct RootView;

impl Render for RootView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(render_stack())
    }
}

#[fennec::command]
fn handle_click(event: &ClickEvent) {
    println!("clicked at position {:?}", event.position());
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(400.), px(300.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|_| RootView),
        )
        .unwrap();
    });
}
