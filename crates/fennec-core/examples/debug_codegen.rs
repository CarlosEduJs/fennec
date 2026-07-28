fn main() {
    let src = r#"---
use fennec_runtime::*;
---

<Stack direction="vertical" gap="12">
    <Text size="xl">Olá mundo</Text>
    <Text>Este é o Fennec 🦊</Text>
    <Button onclick="handle_click">Clique aqui</Button>
</Stack>"#;

    match fennec_core::parser::parse(src) {
        Ok(doc) => {
            println!("=== AST ===");
            println!("{:#?}", doc);
            println!("\n=== GENERATED ===");
            println!("{}", fennec_core::codegen::generate(&doc));
        }
        Err(e) => println!("ERROR: {e}"),
    }
}
