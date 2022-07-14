use serde::{Deserialize, Serialize};

use {
    reqwest::{Request, Response},
    tower::{BoxError, Service},
};

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogResponse {
    pub state: i64,
    pub data: Data,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Data {
    pub products: Vec<Product>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Product {
    pub id: i64,
    pub root: i64,
    pub sizes: Vec<Size>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Extended {
    pub basic_sale: i64,
    pub basic_price_u: i64,
    pub client_sale: i64,
    pub client_price_u: i64,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Color {
    pub name: String,
    pub id: i64,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Size {
    pub option_id: Option<i64>,
    pub stocks: Vec<Stock>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stock {
    pub wh: i64,
    pub qty: i64,
}

pub async fn do_req<C>(mut cli: C, nm_id: i64) -> anyhow::Result<CatalogResponse>
where
    C: Service<Request, Response = Response, Error = BoxError>,
{
    let url = format!(
        r#"https://card.wb.ru/cards/detail?spp=16&regions=76,68,64,83,4,38,80,33,70,82,86,75,30,69,22,66,31,48,1,40,71&stores=117673,122258,122259,125238,125239,125240,6159,507,3158,117501,120602,120762,6158,121709,124731,159402,2737,130744,117986,1733,686,132043&pricemarginCoeff=1.0&reg=1&appType=1&emp=1&locale=ru&lang=ru&curr=rub&couponsGeo=12,3,18,15,21&dest=-1029256,-102269,-1252558,-446087&nm={}"#,
        nm_id
    );
    // println!("{}", url);
    let resp = cli
        .call(reqwest::Client::new().get(&url).build()?)
        .await
        .map_err(|err| anyhow::anyhow!("url={url} - err={err}"))?;

    let catalog_response: CatalogResponse = resp.json().await?;
    // println!("{:?}", catalog_response);
    Ok(catalog_response)
}
