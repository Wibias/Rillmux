/// Guards the release profile: a shipped binary must still produce
/// symbolized backtraces for native Sentry reports. `strip = "symbols"`
/// would make this test fail; `strip = "debuginfo"` keeps it green.
/// The test harness inlines the test body into its `main`, so assert on the
/// crate symbol rather than the individual function name.
#[test]
fn release_backtrace_retains_function_names() {
    std::env::set_var("RUST_BACKTRACE", "1");
    let trace = std::backtrace::Backtrace::force_capture().to_string();
    assert!(
        trace.contains("release_backtrace_contract"),
        "release backtrace lost symbol names:\n{trace}"
    );
}
