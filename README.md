# Rust Playground

Miscellaneous rust services written as part of learning the language

## [Auth](services/auth)

A DynamoDB backed, Rocket OAuth service able to issue JWTs and store hashed credentials using ring. Exposes prometheus metrics.

## [Calculator](services/calculator/calculator)

A pair of authenticated Rocket services that can evaluate arbitrary mathematical expressions. Tests parsing strings using nom, self-recursive async functions and request fanout. Exposes prometheus metrics.

## [Crawler](services/crawler)

A pair of services providing an asynchronous crawler using RabbitMQ as a queue and DynamoDB for persistence. Uses Actix-web and publishes metrics using StatsD.

## [Kinesis Producer](services/kinesis/producer)

An HTTP -> Kinesis service with support for [record aggregation](https://github.com/awslabs/kinesis-aggregation) and batching calls to the PutRecords API.
