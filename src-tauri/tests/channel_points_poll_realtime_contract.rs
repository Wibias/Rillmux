#[test]
fn poll_and_prediction_subscriptions_are_acknowledged_before_ready() {
    let source = include_str!("../src/channel_points_realtime.rs");

    let subscribe = source
        .find("subscribe_poll_topics(&mut socket, &desired.channel_ids).await")
        .expect("missing poll/prediction subscription step");
    let mark_ready = source
        .find("mark_ready(generation);")
        .expect("missing realtime ready transition");
    assert!(
        subscribe < mark_ready,
        "poll/prediction subscriptions must be attempted before realtime is marked ready"
    );

    let function_start = source
        .find("async fn subscribe_poll_topics")
        .expect("missing poll/prediction subscription helper");
    let function_tail = &source[function_start..];
    let function_end = function_tail
        .find("\n}\n\nfn json_value")
        .expect("could not bound poll/prediction subscription helper");
    let function = &function_tail[..function_end];
    assert!(
        function.contains("wait_for_subscriptions"),
        "poll/prediction subscription responses must be checked instead of fire-and-forget"
    );
    assert!(
        function.contains("Result<(), String>"),
        "poll/prediction subscription failures must be observable"
    );
}
