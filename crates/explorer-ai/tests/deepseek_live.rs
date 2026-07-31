use std::{
    task::{Context, Poll, Waker},
    thread,
    time::Duration,
};

use explorer_ai::{AiClient, AiOperation, AiRequest, DeepSeekClient};

#[test]
#[ignore = "requires DEEPSEEK_API_KEY and performs a billable live request"]
fn deepseek_v4_flash_returns_a_short_summary() {
    let key = std::env::var("DEEPSEEK_API_KEY").expect("DEEPSEEK_API_KEY must be set");
    let client = DeepSeekClient::new(key).expect("client");
    let mut future = client.execute(AiRequest {
        operation: AiOperation::Summarize,
        provider: "deepseek".into(),
        model: "deepseek-v4-flash".into(),
        input: "Lua automation can react to events, run tools, wait asynchronously, and write summaries.".into(),
        system_prompt: Some("Summarize the input in one Traditional Chinese sentence.".into()),
        timeout_ms: 60_000,
        correlation_id: "live-smoke".into(),
    });
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    for _ in 0..1_200 {
        if let Poll::Ready(result) = future.as_mut().poll(&mut context) {
            let response = result.expect("live DeepSeek response");
            assert_eq!(response.model, "deepseek-v4-flash");
            assert!(!response.text.trim().is_empty());
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("live DeepSeek smoke test did not complete in time");
}
