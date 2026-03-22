#!/bin/sh
read input
a=$(echo "$input" | sed 's/.*"a":\(-\?[0-9.]*\).*/\1/')
b=$(echo "$input" | sed 's/.*"b":\(-\?[0-9.]*\).*/\1/')
op=$(echo "$input" | sed 's/.*"op":"\([^"]*\)".*/\1/')

if [ "$op" = "add" ]; then
    result=$(echo "$a + $b" | bc -l)
elif [ "$op" = "multiply" ]; then
    result=$(echo "$a * $b" | bc -l)
else
    echo "{\"event\":\"error\",\"data\":\"unknown op: $op\"}"
    exit 1
fi

echo "{\"result\":$result}"
