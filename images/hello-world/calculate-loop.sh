#!/bin/sh
while IFS= read -r line; do
    a=$(echo "$line" | sed 's/.*"a":\(-\?[0-9.]*\).*/\1/')
    b=$(echo "$line" | sed 's/.*"b":\(-\?[0-9.]*\).*/\1/')
    op=$(echo "$line" | sed 's/.*"op":"\([^"]*\)".*/\1/')
    if [ "$op" = "add" ]; then
        result=$(echo "$a + $b" | bc -l)
    elif [ "$op" = "multiply" ]; then
        result=$(echo "$a * $b" | bc -l)
    else
        echo "{\"event\":\"error\",\"data\":\"unknown op: $op\"}"
        continue
    fi
    echo "{\"result\":$result}"
done
