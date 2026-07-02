# Benchmarks (docs/profiling-plan.md §5): release build, warmup + median-of-5,
# GST ratio column, history in bench/history.csv, counter tables in
# bench/results/. Run a subset with: make bench BENCH_ARGS="send_loop"
bench:
	bash bench/run.sh $(BENCH_ARGS)

.PHONY: bench
