fn main() {
    let src = r#"---
use fncc_runtime::*;
---

<Stack direction="vertical" gap="12">
    <Text size="xl">Olá mundo</Text>
    <Text>Este é o fncc 🦊</Text>
    <Button onclick="handle_click">Clique aqui</Button>
</Stack>"#;

    match fncc_core::parser::parse(src) {
        Ok(doc) => {
            println!("=== AST ===");
            println!("{:#?}", doc);
            println!("\n=== GENERATED ===");
            println!("{}", fncc_core::codegen::generate(&doc));
        }
        Err(e) => println!("ERROR: {e}"),
    }
}
