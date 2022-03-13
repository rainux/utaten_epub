use anyhow::Result;
use kuchiki::{traits::TendrilSink, NodeRef};
use std::fs::{self, File};
use std::io::{self, BufRead};
use std::path::Path;

fn main() -> Result<()> {
    fs::create_dir_all("lyrics")?;

    if let Ok(lines) = read_lines("songs") {
        for line in lines {
            if let Ok(song) = line {
                if let Some(url) = search_song(&song)? {
                    println!("{}", url);
                    download_lyric(&url, &song)?;
                } else {
                    println!("Not found: {}", song);
                }
            }
        }
    }

    Ok(())
}

fn search_song(song: &str) -> Result<Option<String>> {
    let (title, artist) = song.split_once(" / ").unwrap_or_else(|| (song, ""));

    let body = reqwest::blocking::Client::new()
        .get("https://utaten.com/lyric/search")
        .query(&[("artist_name", artist), ("title", title)])
        .send()?
        .text()?;

    let document = kuchiki::parse_html().one(body);

    if let Some(link) = document.select(".searchResult__title a").unwrap().next() {
        let attrs = link.as_node().as_element().unwrap().attributes.borrow();
        let path = attrs.get("href").unwrap();
        let url = format!("https://utaten.com{}", path);
        Ok(Some(url))
    } else {
        Ok(None)
    }
}

fn download_lyric(url: &str, song: &str) -> Result<()> {
    let body = reqwest::blocking::Client::new().get(url).send()?.text()?;

    let document = kuchiki::parse_html().one(body);
    let lyric_title = extract_lyric_title(&document);
    let lyric_data = extract_lyric_data(&document);
    let lyric_body = extract_lyric_body(&document);

    let article = document.select("article").unwrap().next().unwrap();
    let article = article.as_node();
    article.children().for_each(|c| c.detach());

    article.append(lyric_title);
    article.append(lyric_data);
    article.append(lyric_body);

    let mut html = Vec::new();
    article.serialize(&mut html)?;

    let filename = format!("lyrics/{}.html", song.replace(" / ", " - "));
    fs::write(filename, html)?;

    Ok(())
}

fn extract_lyric_title(document: &NodeRef) -> NodeRef {
    let lyric_title = document.select(".newLyricTitle").unwrap().next().unwrap();
    let lyric_title = lyric_title.as_node();
    // Remove "の歌詞" in title
    lyric_title
        .select(".newLyricTitle_afterTxt")
        .unwrap()
        .next()
        .unwrap()
        .as_node()
        .detach();
    lyric_title.to_owned()
}

fn extract_lyric_data(document: &NodeRef) -> NodeRef {
    let lyric_data = document.select(".lyricData").unwrap().next().unwrap();
    let lyric_data = lyric_data.as_node();
    // # Remove tags and action buttons
    lyric_data
        .select(".newLyricWorkFooter")
        .unwrap()
        .next()
        .unwrap()
        .as_node()
        .detach();
    // Fix relative links
    lyric_data
        .select(".newLyricWork a")
        .unwrap()
        .for_each(|link| {
            let mut attrs = link.as_node().as_element().unwrap().attributes.borrow_mut();
            if let Some(href) = attrs.get_mut("href") {
                *href = format!("https://utaten.com{}", href);
            }
        });
    lyric_data.to_owned()
}

fn extract_lyric_body(document: &NodeRef) -> NodeRef {
    let lyric_body = document.select(".lyricBody").unwrap().next().unwrap();
    let lyric_body = lyric_body.as_node();
    // Remove romaji part
    lyric_body
        .select(".romaji")
        .unwrap()
        .next()
        .unwrap()
        .as_node()
        .detach();
    lyric_body.to_owned()
}

fn read_lines<P>(filename: P) -> io::Result<io::Lines<io::BufReader<File>>>
where
    P: AsRef<Path>,
{
    let file = File::open(filename)?;
    Ok(io::BufReader::new(file).lines())
}
