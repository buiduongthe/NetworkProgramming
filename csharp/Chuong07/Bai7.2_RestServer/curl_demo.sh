#!/bin/sh
# Bài 7.2 – Thử server REST bằng curl (in lệnh trước khi chạy). Dùng: sh curl_demo.sh [port]
P=${1:-8080}
U="http://127.0.0.1:$P/api/v1/terms"
echo "\$ curl -i $U/UDP"
curl -s -i "$U/UDP" | tr -d '\r'; echo; echo
echo "\$ curl -i -H 'Transfer-Encoding: chunked' -d '{\"term\":\"A\",\"text\":\"B\"}' $U"
curl -s -i -H 'Transfer-Encoding: chunked' -d '{"term":"A","text":"B"}' "$U" | tr -d '\r'; echo; echo
echo "\$ curl -i -X POST -H 'Content-Length: 4000000000' $U"
curl -s -i -X POST -H 'Content-Length: 4000000000' "$U" | tr -d '\r'; echo
