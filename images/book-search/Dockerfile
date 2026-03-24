FROM potato-base
RUN apt-get update && apt-get install -y curl jq && rm -rf /var/lib/apt/lists/*
RUN curl -sL "https://www.gutenberg.org/cache/epub/11/pg11.txt" -o /book.txt
COPY index.html /app/gui/index.html
COPY search.sh.html /app/templates/search.sh.html
COPY search.sh /app/bin/search.sh
RUN chmod +x /app/bin/search.sh
