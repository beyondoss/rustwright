# rustwright

Idiomatic **native Rust API** for the [Rustwright](https://github.com/beyondoss/rustwright)
Chromium CDP engine. This crate is the in-process facade used by
`rustwright-mcp` and `rustwright-cli`.

```rust
use rustwright::{chromium, LaunchOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let browser = chromium().launch(LaunchOptions::default())?;
    let page = browser.new_page()?;
    page.goto("https://example.com", None)?;
    println!("{}", page.title(None)?);
    browser.close()?;
    Ok(())
}
```

Alpha; Chromium-only. For agent use, prefer the [MCP server](../mcp/README.md)
or [CLI](../cli/README.md).

## License

[MIT](https://github.com/beyondoss/rustwright/blob/main/LICENSE)
