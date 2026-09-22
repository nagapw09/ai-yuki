//! Explicit integration check; no personal conversation data or credentials logged.
use base64::Engine;
use yuki_ai::{
    cli::CliProvider, ChatRequest, ContentBlock, Message, NullSink, Provider, ProviderKind,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let kind = if args.iter().any(|a| a == "claude") {
        ProviderKind::ClaudeCli
    } else {
        ProviderKind::CodexCli
    };
    let provider = CliProvider::new(kind);
    provider.list_models().await?;
    println!("CLI authentication verified");
    if let Some(at) = args.iter().position(|a| a == "--image") {
        let bytes = std::fs::read(args.get(at + 1).ok_or("Image path required")?)?;
        let mut message = Message::user(
            "What is the large word in the image? Reply with only that word, no tools.",
        );
        message.content.push(ContentBlock::Image {
            media_type: "image/png".into(),
            data: base64::engine::general_purpose::STANDARD.encode(bytes),
        });
        let response = provider
            .chat(
                &ChatRequest {
                    model: "default".into(),
                    system: None,
                    messages: vec![message],
                    tools: vec![],
                    max_tokens: Some(100),
                    temperature: None,
                },
                &NullSink,
            )
            .await?;
        if !response.text().contains("ORCHID") {
            return Err(format!("Unexpected vision response: {}", response.text()).into());
        }
        println!("VISION_OK: synthetic image recognized");
    }
    if args.iter().any(|a| a == "--chat") {
        let response = provider
            .chat(
                &ChatRequest {
                    model: "default".into(),
                    system: Some("You are Yuki. This is an integration test.".into()),
                    messages: vec![Message::user(
                        "Reply with the single word YUKI_OK. Do not use tools.",
                    )],
                    tools: vec![],
                    max_tokens: Some(100),
                    temperature: None,
                },
                &NullSink,
            )
            .await?;
        if !response.text().contains("YUKI_OK") {
            return Err("Unexpected smoke test response".into());
        }
        println!("YUKI_OK: structured response verified");
    }
    Ok(())
}
