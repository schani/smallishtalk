# Run ALL tests (what CI runs). `cargo test` covers the Rust/VM tests, the
# in-image Smalltalk suite (tests/st_suite.rs launches st/tests/ui/ on the
# VM), and the bootstrap fixpoint (tests/bootstrap_test.rs); run-st-tests.sh
# runs the GST-hosted compiler suites (st/tests/) under GNU Smalltalk.
test:
	cargo test
	./run-st-tests.sh

# Benchmarks (docs/profiling-plan.md §5): release build, warmup + median-of-5,
# GST ratio column, history in bench/history.csv, counter tables in
# bench/results/. Run a subset with: make bench BENCH_ARGS="send_loop"
bench:
	bash bench/run.sh $(BENCH_ARGS)

# Run the classic Smalltalk UI (UI.md) from a fresh checkout.
#   make ui            headless: render a Class Browser -> ui-screenshot.png
#   make ui-window     live, click-navigable window (needs the `ui` feature + a display)
#   make ui-workspace  live Workspace window: edit, drag-select, right-click for
#                      do it / print it
ui:
	bash scripts/run-ui.sh

ui-window:
	bash scripts/run-ui.sh --window

ui-workspace:
	bash scripts/run-ui.sh --workspace

.PHONY: test bench ui ui-window ui-workspace
