# Calculator

A pair of Rocket services to test request fanout

- [calculator](./calculator) - an incredibly simple service that can perform a mathematical operation on two numbers
- [gateway](./gateway) - a gateway service that uses the calculator API to provide computation of arbitrary mathematical expressions 

## Setup

As these APIs require valid JWTs you will need to first follow the instructions [here](../auth) to setup and run an auth service.
