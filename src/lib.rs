use cssparser::ParseError;

use scraper::{Html, Selector};
use selectors::parser::SelectorParseErrorKind;
use serde::{Deserialize, Serialize};

use std::collections::btree_map::{BTreeMap, Entry};

const BASE_URL: &str = "https://dle.rae.es";

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(untagged)]
pub enum Value {
    List(Vec<String>),
    Unique(String),
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(untagged)]
pub enum ValueVariant {
    String(String),
    Map(BTreeMap<String, Value>),
    List(Vec<String>),
}

#[derive(Debug)]
pub enum WebScrapError<'a> {
    ParseError(ParseError<'a, SelectorParseErrorKind<'a>>),
    NotFound,
    Other(String),
}

impl<'a> From<ParseError<'a, SelectorParseErrorKind<'a>>> for WebScrapError<'a> {
    fn from(err: ParseError<'a, SelectorParseErrorKind<'a>>) -> WebScrapError<'a> {
        WebScrapError::ParseError(err)
    }
}

pub fn search<'a>(html_doc: &str) -> Result<BTreeMap<String, ValueVariant>, WebScrapError<'a>> {
    let html = Html::parse_document(html_doc);
    let mut dicc: BTreeMap<String, ValueVariant> = BTreeMap::new();
    // Obtain the div that contains the results
    let selector_results = Selector::parse("div#resultados")?;
    let results = html
        .select(&selector_results)
        .next()
        .expect("Failed to get the results");
    let results_text: Vec<&str> = results.text().collect();

    let a_selector = Selector::parse("a")?;

    // Check if word exists
    if results_text.contains(&" La entrada que se muestra a continuación podría estar relacionada:")
    {
        let word: &str = {
            let word = results
                .select(&a_selector)
                .next()
                .expect("Couldn't obtain the related word")
                .text()
                .next()
                .unwrap();
            if word.contains(',') {
                word.split(',').next().unwrap()
            } else {
                word
            }
        };
        Err(WebScrapError::Other(word.to_owned()))
    } else if results_text.contains(&"Aviso: ") {
        Err(WebScrapError::NotFound)
    } else {
        let articles_selector = Selector::parse("article")?;
        let ps_selector = Selector::parse("p")?;

        for (i, result) in results.select(&articles_selector).enumerate() {
            if result.first_child().unwrap().value().as_element().is_some() {
                continue;
            }
            let i = i + 1;
            dicc.insert(i.to_string(), ValueVariant::Map(BTreeMap::new()));
            let mut complex_form = String::new();
            for element in result.select(&ps_selector) {
                let paragraph_class: &str = element
                    .value()
                    .attr("class")
                    .expect("Paragraph without classes");
                let p_text = element.text().collect::<Vec<&str>>().join("");

                if paragraph_class.contains('j') {
                    let (meaning_number, meaning_text) =
                        match p_text.splitn(2, '.').collect::<Vec<&str>>().as_slice() {
                            [first, second] => (*first, *second),
                            _ => unreachable!(),
                        };
                    if let ValueVariant::Map(map) =
                        dicc.get_mut(&i.to_string()).expect("Failed to get the map")
                    {
                        map.insert(
                            meaning_number.to_string(),
                            Value::Unique(meaning_text.trim_start().to_string()),
                        );
                    };
                } else if ["k5", "k6"].contains(&paragraph_class) {
                    complex_form = p_text;
                    if let ValueVariant::Map(map) =
                        dicc.get_mut(&i.to_string()).expect("Failed to get the map")
                    {
                        map.insert(complex_form.to_string(), Value::List(Vec::new()));
                    };
                } else if paragraph_class == "m" {
                    if let ValueVariant::Map(map) =
                        dicc.get_mut(&i.to_string()).expect("Failed to get the map")
                    {
                        if let Value::List(vec) = map
                            .get_mut(&complex_form)
                            .expect("Failed to get the vector")
                        {
                            vec.push(p_text);
                        }
                    }
                } else if paragraph_class == "l2" {
                    let link = element
                        .select(&a_selector)
                        .next()
                        .expect("Failed to get the link");
                    let link_href = link.value().attr("href").expect("Failed to get the href");
                    let mut link_text = link.text().collect::<Vec<&str>>().join("");

                    let redirect_link = format!("{}{}", BASE_URL, link_href);

                    // If any of the complex forms' array is empty,
                    // it means this 'also see' belongs to the complex form
                    let mut loop_breaks = false;
                    let v = if let ValueVariant::Map(map) = dicc[&i.to_string()].clone() {
                        map.into_iter()
                    } else {
                        unreachable!();
                    };

                    for (key, value) in v {
                        if matches!(value, Value::List(vec) if vec.is_empty()) {
                            if let ValueVariant::Map(map) =
                                dicc.get_mut(&i.to_string()).expect("Failed to get the map")
                            {
                                if let Value::List(vec) =
                                    map.get_mut(&key).expect("Failed to get the vector")
                                {
                                    vec.push(format!("Véase '{}' ({} )", link_text, redirect_link));
                                }
                            };
                            loop_breaks = true;
                            break;
                        }
                    }

                    if !loop_breaks {
                        let superscript = link.select(&Selector::parse("sup")?).next();
                        let superscript_text = if let Some(element) = superscript {
                            element.text().collect::<Vec<&str>>().join("")
                        } else {
                            String::new()
                        };

                        if superscript_text.is_empty() {
                            link_text = link_text.replace(&superscript_text, "");
                        }

                        match dicc.entry(i.to_string()) {
                            Entry::Occupied(mut entry) => match entry.get_mut() {
                                ValueVariant::Map(map) => {
                                    map.insert(
                                        "Véase también".to_string(),
                                        Value::Unique(format!(
                                            "'{}' ({})",
                                            link_text, redirect_link
                                        )),
                                    );
                                }
                                _ => unreachable!(),
                            },
                            Entry::Vacant(entry) => {
                                entry.insert(ValueVariant::String(format!(
                                    "Véase '{}' ({})",
                                    link_text, redirect_link
                                )));
                            }
                        };
                    }
                } else if paragraph_class.contains('l') {
                    let direction = element
                        .select(&a_selector)
                        .next()
                        .unwrap()
                        .value()
                        .attr("href")
                        .unwrap();
                    let redirect_link = format!("{}{}", BASE_URL, direction);

                    if let ValueVariant::List(vec) = dicc
                        .entry("Envíos".to_string())
                        .or_insert_with(|| ValueVariant::List(Vec::new()))
                    {
                        vec.push(redirect_link);
                    }
                }
            }
        }
        Ok(dicc)
    }
}
