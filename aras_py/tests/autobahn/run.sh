# make sure the websocket server is running at 0.0.0.0:9001

docker run -it --rm \
  -v $(pwd)/config/fuzzingclient.json:/config/fuzzingclient.json \
  -v $(pwd)/autobahn_reports:/autobahn_reports \
  crossbario/autobahn-testsuite \
  wstest -m fuzzingclient -s /config/fuzzingclient.json
