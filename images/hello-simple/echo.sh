#!/bin/sh
while IFS= read -r line; do
    text=$(echo "$line" | sed 's/.*"text":"\([^"]*\)".*/\1/')
    echo "$text" | tr '[:lower:]' '[:upper:]'
done
