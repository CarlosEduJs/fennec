use fncc_core::parser;

fn main() {
    let sources = vec![
        r#"<Text size="xl">hello</Text>"#,
        r#"<Stack direction="vertical"><Text>oi</Text></Stack>"#,
        "---\nuse crate::foo;\n---\n<App></App>",
        r#"<Text>{state.msg}</Text>"#,
        r#"<Button onclick="handle_click">Click</Button>"#,
    ];

    for src in sources {
        println!("=== INPUT ===");
        println!("{src}");
        println!("=== AST ===");
        match parser::parse(src) {
            Ok(doc) => println!("{:#?}", doc),
            Err(e) => println!("ERROR: {e}"),
        }
        println!();
    }
}
