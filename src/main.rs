extern crate serde_json;
use chrono::{DateTime, Duration, TimeZone, Utc};
use inquire::RangeSelect;
use reqwest::{header::CONTENT_TYPE, Client, Method};
use serde::{de, Deserialize, Serialize};
use std::{
    fmt::{self, Display},
    fs::{self, File},
};
use tokio;

const BASE_URL: &str = "https://api.track.toggl.com/api/v9/";

#[allow(unused)]
#[derive(Deserialize, Debug)]
struct TimeEntry {
    start: DateTime<Utc>,
    #[serde(deserialize_with = "deserialize_duration")]
    duration: Duration,
    description: Option<String>,
    project_id: Option<i64>,
    stop: Option<DateTime<Utc>>,
    id: u64,
}

fn deserialize_duration<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: de::Deserializer<'de>,
{
    struct DurationVisitor;
    impl<'de> de::Visitor<'de> for DurationVisitor {
        type Value = Duration;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("an i64 which is duration in seconds")
        }

        fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(Duration::seconds(v))
        }

        fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E> {
            Ok(Duration::seconds(v as i64))
        }
    }
    deserializer.deserialize_i64(DurationVisitor)
}

impl Display for TimeEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut date = Utc.ymd(2000, 1, 1).and_hms(0, 0, 0);
        date += self.duration;
        write!(
            f,
            "{}: {} - {}",
            self.start.format("%F"),
            date.format("%H:%M:%S"),
            self.description
                .as_ref()
                .unwrap_or(&"no description".to_string())
        )
    }
}

struct Summary {
    duration: chrono::Duration,
}

impl Summary {
    fn new(duration: chrono::Duration) -> Self {
        Self { duration }
    }
}

impl Display for Summary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let full_duration = self.duration;
        let hours = full_duration.num_seconds() / 60i64.pow(2);
        let minutes = full_duration.num_seconds() % 60i64.pow(2) / 60;
        let seconds = full_duration.num_seconds() % 60i64.pow(2) % 60;
        let per_hour = 26.;
        let minutes = minutes as f32;
        let hours = hours as f32;
        let money = (hours * per_hour) + per_hour / 60. * minutes;

        write!(
            f,
            "Total time: {hours:02}:{minutes:02}:{seconds:02} which is worth {money:.2}€"
        )
    }
}

struct ToggleClient {
    client: Client,
    api_key: String,
}

impl ToggleClient {
    pub fn new(api_key: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
        }
    }

    pub fn auth_request(&self, method: Method, uri: &str) -> reqwest::RequestBuilder {
        self.client
            .request(method, format!("{}{}", BASE_URL, uri))
            .header(CONTENT_TYPE, "application/json")
            .basic_auth(&self.api_key, Some("api_token"))
    }
}

#[derive(Deserialize, Serialize)]
struct Config {
    api_key: String,
}

impl Config {
    fn new(api_key: String) -> Self {
        Self { api_key }
    }
}

fn fold_options(elements: &[&TimeEntry]) -> Summary {
    let full_duration = elements
        .iter()
        .map(|dated_dur| dated_dur.duration)
        .fold(Duration::zero(), |cum, element| cum + element);
    Summary::new(full_duration)
}

fn load_or_ask_api_key() -> Config {
    match File::open("./api_key.json") {
        Ok(reader) => serde_json::from_reader(reader).unwrap(),
        Err(_) => {
            let config = Config::new(
                inquire::Text::new("Please enter your api key:")
                    .prompt()
                    .unwrap(),
            );
            File::create("./api_key.json").unwrap();
            fs::write("./api_key.json", serde_json::to_string(&config).unwrap()).unwrap();
            config
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), reqwest::Error> {
    let toggle_client = ToggleClient::new(load_or_ask_api_key().api_key);

    let end_date = Utc::now();
    let start_date = end_date - Duration::days(89);

    print!(
        "Showing entries from {} to {} ",
        start_date.format("%F"),
        end_date.format("%F")
    );

    let toggle_times: Vec<TimeEntry> = toggle_client
        .auth_request(Method::GET, "me/time_entries")
        .query(&[
            ("start_date", start_date.format("%F").to_string()),
            ("end_date", end_date.format("%F").to_string()),
        ])
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    println!("({})", toggle_times.len());
    let summary = RangeSelect::new("Which times should be sum up?", toggle_times, &fold_options)
        .with_page_size(20)
        .prompt_skippable();

    match summary {
        Ok(Some(choice)) => println!("{}", choice),
        Ok(_) => println!("You cancelled it stupid fuk"),
        Err(_) => println!("You stupid cunt fucked up"),
    }

    Ok(())
}
