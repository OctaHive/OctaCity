#!/bin/sh
set -eu
IFS= read -r frame
printf '%s\n' "$frame" > captured-request.json
if [ -f hold ]; then
  : > ready
  IFS= read -r cancellation
  printf '%s\n' "$cancellation" > cancellation.json
  sleep 30 &
  wait
fi
cat response.json
printf '\n'
