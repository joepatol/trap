Run a throughput benchmark comparing aras against uvicorn and display a results table.

Follow these steps exactly:

## 1. Check prerequisites

Check whether `wrk` is installed:
```bash
which wrk
```
If `wrk` is not found, stop and tell the user:
> `wrk` is not installed ask the user if you should install it

Check whether uvicorn is available in the venv:
```bash
/Users/joepatol/Documents/dev/trap/aras_py/.venv/bin/python -c "import uvicorn"
```
If missing, ask the user if okay and then install it:
```bash
/Users/joepatol/Documents/dev/trap/aras_py/.venv/bin/pip install uvicorn --quiet
```

## 2. Start the servers

Background processes in a single shell session are scheduled differently than foreground processes and produce unreliable benchmark results. Open each server in its own Terminal window using `osascript` so they run as foreground processes.

Start aras in a new Terminal window:
```bash
osascript -e 'tell application "Terminal" to do script "cd /Users/joepatol/Documents/dev/trap/aras_py && PYTHONPATH=python:. .venv/bin/aras serve tests.apps.performance.main:app --port 8081"'
```

Start uvicorn in a new Terminal window:
```bash
osascript -e 'tell application "Terminal" to do script "cd /Users/joepatol/Documents/dev/trap/aras_py && PYTHONPATH=python:. .venv/bin/uvicorn tests.apps.performance.main:app --port 8082 --log-level error"'
```

Wait for both to be ready by polling `/health_check` on each port (retry for up to 15 seconds):
```bash
for port in 8081 8082; do
  deadline=$((SECONDS + 15))
  until curl -sf "http://127.0.0.1:$port/health_check" > /dev/null 2>&1; do
    [ $SECONDS -ge $deadline ] && echo "TIMEOUT port $port" && exit 1
    sleep 0.3
  done
  echo "port $port ready"
done
```

## 3. Run wrk

Run with 4 threads, 1000 connections, 30-second duration against `http://127.0.0.1:<port>/health_check`.

Benchmark aras:
```bash
wrk -t4 -c1000 -d30s http://127.0.0.1:8081/health_check
```

Benchmark uvicorn:
```bash
wrk -t4 -c1000 -d30s http://127.0.0.1:8082/health_check
```

Close both server Terminal windows when done:
```bash
osascript -e 'tell application "Terminal" to close (every window whose name contains "8081")' && \
osascript -e 'tell application "Terminal" to close (every window whose name contains "8082")'
```

## 4. Parse results

Extract `Requests/sec` from each wrk output. The line looks like:
```
Requests/sec:   8246.78
```

## 5. Persist and display

History file: `aras_py/tests/performance/history.json` (create it if it does not exist).

Append the current result as:
```json
{ "date": "<YYYY-MM-DD HH:MM>", "aras_rps": <integer>, "uvicorn_rps": <integer> }
```

Then print a table of the last 3 stored results plus the current run, with columns: Date · aras RPS · uvicorn RPS · delta (aras vs uvicorn, as a signed percentage). Mark the current run with `<-- current`. Also give a short review on the performance observed, also consider the past performance tests in this review.
