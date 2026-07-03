# Benchmarks (docs/profiling-plan.md §5): release build, warmup + median-of-5,
# GST ratio column, history in bench/history.csv, counter tables in
# bench/results/. Run a subset with: make bench BENCH_ARGS="send_loop"
bench:
	bash bench/run.sh $(BENCH_ARGS)

# Run the classic Smalltalk UI (UI.md) from a fresh checkout.
#   make ui          headless: render a Class Browser -> ui-screenshot.png
#   make ui-window   live, click-navigable window (needs the `ui` feature + a display)
ui:
	bash scripts/run-ui.sh

ui-window:
	bash scripts/run-ui.sh --window

.PHONY: bench ui ui-window
