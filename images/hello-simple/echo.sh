#!/bin/sh
while IFS= read -r line; do
    text=$(echo "$line" | sed 's/.*"text":"\([^"]*\)".*/\1/')
    upper=$(echo "$text" | tr '[:lower:]' '[:upper:]')
    echo "{\"echo\":\"$upper\"}"
done
