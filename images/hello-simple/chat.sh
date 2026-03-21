#!/bin/sh
read input
message=$(echo "$input" | sed 's/.*"message":"\([^"]*\)".*/\1/')

for word in Ok I will $message! Anything else?; do
    echo "{\"event\":\"output\",\"data\":{\"word\":\"$word\"}}"
    sleep 0.1
done

echo "{\"event\":\"output\",\"data\":{\"done\":true}}"
