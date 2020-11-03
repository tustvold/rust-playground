use crate::api::{api_factory, ApiState};
use actix_web::{middleware, web, App, HttpServer};
use shared::dao::LinkDaoDynamo;
use shared::metrics::MetricsService;
use shared::mq::{RabbitMQChannel, RabbitMQConnection};

mod api;

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();
    let config = shared::config::Config::from_env().unwrap();
    let metrics = web::Data::new(MetricsService::new(&config.metrics));
    let connection = RabbitMQConnection::new(&config.rabbit);

    HttpServer::new(move || {
        let dao = Box::new(LinkDaoDynamo::new(&config.dynamo));
        let publisher = Box::new(RabbitMQChannel::new(&connection.clone()));

        App::new()
            .wrap(middleware::Logger::default())
            .data(ApiState::new(dao, publisher))
            .app_data(metrics.clone())
            .configure(api_factory)
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
