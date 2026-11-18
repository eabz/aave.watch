//! ClickHouse over its HTTP interface. Rows come back as JSON objects and are passed through, so a
//! new column in a table reaches the API without a code change.

use serde_json::Value;

pub struct ClickHouse {
    url: String,
    http: reqwest::Client,
}

impl ClickHouse {
    pub fn new(url: String) -> Self {
        Self { url, http: reqwest::Client::new() }
    }

    /// Writes rows as JSON lines. Used only by `load-registry`; the serving API never writes.
    pub async fn insert(&self, table: &str, rows: &[Value]) -> Result<(), String> {
        let body = rows.iter().map(Value::to_string).collect::<Vec<_>>().join("\n");
        self.send(&format!("INSERT INTO {table} FORMAT JSONEachRow"), body).await
    }

    pub async fn command(&self, sql: &str) -> Result<(), String> {
        let res = self.http.post(&self.url).body(sql.to_string()).send().await
            .map_err(|e| format!("ClickHouse is unreachable: {e}"))?;
        if res.status().is_success() { return Ok(()); }
        Err(format!("ClickHouse refused {sql}: {}", res.text().await.unwrap_or_default().lines().next().unwrap_or("")))
    }

    async fn send(&self, query: &str, body: String) -> Result<(), String> {
        let res = self.http.post(&self.url).query(&[("query", query)]).body(body).send().await
            .map_err(|e| format!("ClickHouse is unreachable: {e}"))?;
        if res.status().is_success() { return Ok(()); }
        let text = res.text().await.unwrap_or_default();
        Err(format!("ClickHouse refused {query}: {}", text.lines().next().unwrap_or("")))
    }

    /// Runs one SELECT. `params` fill the `{name:Type}` placeholders; values never enter the SQL
    /// text, so a parameter cannot change the query.
    pub async fn query(&self, sql: &str, params: &[(&str, String)]) -> Result<Vec<Value>, String> {
        let mut q: Vec<(String, String)> = vec![
            ("default_format".into(), "JSON".into()),
            // 64-bit and wider integers as strings: they do not fit a JS number
            ("output_format_json_quote_64bit_integers".into(), "1".into()),
            ("max_execution_time".into(), "10".into()),
        ];
        q.extend(params.iter().map(|(k, v)| (format!("param_{k}"), v.clone())));
        let res = self.http.post(&self.url).query(&q).body(sql.to_string()).send().await
            .map_err(|e| format!("ClickHouse is unreachable: {e}"))?;
        let ok = res.status().is_success();
        let text = res.text().await.map_err(|e| format!("ClickHouse answer could not be read: {e}"))?;
        if !ok {
            let first = text.lines().next().unwrap_or("").chars().take(300).collect::<String>();
            return Err(format!("ClickHouse refused the query: {first}"));
        }
        let mut body: Value = serde_json::from_str(&text).map_err(|e| format!("ClickHouse answer is not JSON: {e}"))?;
        match body.get_mut("data").map(Value::take) {
            Some(Value::Array(rows)) => Ok(rows),
            _ => Err("ClickHouse answer has no data".into()),
        }
    }
}
