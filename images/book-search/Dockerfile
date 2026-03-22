FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y curl jq && rm -rf /var/lib/apt/lists/*
RUN curl -sL "https://www.gutenberg.org/cache/epub/11/pg11.txt" -o /book.txt
COPY index.html /index.html
COPY search.html /search.html
COPY search.sh /search.sh
RUN chmod +x /search.sh
