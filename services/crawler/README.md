# Crawler

A distributed web crawler that recursively crawls a website and builds a sitemap of its links to other pages.

## Architecture

The architecture of the crawler is very simple, consisting of an api server and crawler nodes communicating over a RabbitMQ queue. The server and crawler nodes share a DynamoDB table mapping URLs to a list of URLs they link to.

The RabbitMQ messages consist of a JSON encoded payload containing the url to crawl. The crawler nodes does the following pseudocode

```
loop {
    req = rabbitmq.pop()
    if dynamo.exists(req.url):
        continue
    body = http.get(req.url)
    links = parseBody(body)
    for link in links:
        if shouldCrawl(link) and not dynamo.exists(link):
            rabbitmq.enqueue(link)
    dynamo.set(req.url, links)
    rabbitmq.ack(req)
}
```

This will potentially crawl the same URL multiple times but this is acceptable. 

Traditionally distributed crawlers might separate the downloading and parsing concerns, however, in this case the parsing logic is so simple as to render this an unnecessary overhead.
