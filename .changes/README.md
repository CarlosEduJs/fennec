# Gerenciamento de Mudanças (.changes/)

Este diretório contém os arquivos de declaração de mudança de versão para os crates do **Fennec**.

## Como adicionar uma mudança num Pull Request:

Você pode utilizar a ferramenta `cargo xtask`:

```bash
cargo xtask change
```

Ou criar manualmente um arquivo `.md` neste diretório (ex: `.changes/minha-feature.md`) com o seguinte formato:

```markdown
---
fennec-runtime: minor
fennec: patch
---

- Descrição da nova funcionalidade ou correção para o CHANGELOG.
```

### Tipos de Bump:
- **`patch`**: Correções de bugs sem quebra de compatibilidade.
- **`minor`**: Novas funcionalidades mantendo retrocompatibilidade.
- **`major`**: Alterações que quebram a API (Breaking Changes).
