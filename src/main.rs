use anyhow::{bail, Result};
use html5ever::{interface::QualName, local_name, namespace_url, ns};
use kuchiki::{traits::TendrilSink, Attribute, ExpandedName, NodeRef};
use std::fs::{self, File};
use std::io::{self, BufRead};
use std::path::Path;
use std::process;

fn main() -> Result<()> {
    fs::create_dir_all("lyrics")?;

    if !Path::new("songs").exists() {
        println!(
            "This utility can download lyrics of your favorite Japanese songs from https://utaten.com/\n\
            and build them into a EPUB e-book.\n\n\
            Create a `songs` file with the song names, one per line, and run this utility again.\n\
            Optionally, you can append artist name to the song name, separated by a slash."
        );
        process::exit(1);
    }

    let songs = read_lines("songs")?
        .map(|line| {
            if let Ok(song) = line {
                let filename = lyric_filename(&song);
                if Path::new(&filename).exists() {
                    println!("Skipping {}, lyric already downloaded", song);
                    return Ok(Some(filename));
                }
                if let Some(url) = search_song(&song)? {
                    let filename = download_lyric(&url, &song)?;
                    Ok(Some(filename))
                } else {
                    println!("Not found");
                    Ok(None)
                }
            } else {
                bail!("Invalid songs file, ensure it is UTF-8 encoded.");
            }
        })
        .collect::<Result<Vec<Option<_>>>>()?;

    let songs = songs.into_iter().filter_map(|x| x).collect::<Vec<_>>();

    if songs.is_empty() {
        println!("\nNo songs found, please add some valid title to songs file.");
        process::exit(1);
    }

    println!("\nBuilding lyrics.epub");
    let status = process::Command::new("pandoc")
        .args(["--toc", "--metadata-file=lyrics.yaml", "-f", "html"])
        .args(songs)
        .args([
            "--css",
            "styles.css",
            "--epub-embed-font=utIcon.ttf",
            "-o",
            "lyrics.epub",
        ])
        .status()?;

    process::exit(status.code().unwrap_or(0));
}

fn search_song(song: &str) -> Result<Option<String>> {
    println!("Searching for {}", song);
    let (title, artist) = song.split_once("/").unwrap_or_else(|| (song, ""));
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

fn lyric_filename(song: &str) -> String {
    format!("lyrics/{}.html", song.replace(" / ", " - "))
}

fn download_lyric(url: &str, song: &str) -> Result<String> {
    println!("Downloading lyric for {}", song);
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

    let page_break = NodeRef::new_element(
        QualName::new(None, ns!(html), local_name!("div")),
        [(
            ExpandedName::new("", local_name!("class")),
            Attribute {
                prefix: None,
                value: "page-break".to_string(),
            },
        )],
    );
    article.append(page_break);

    let mut html = Vec::new();
    article.serialize(&mut html)?;

    let filename = lyric_filename(song);
    fs::write(&filename, html)?;

    Ok(filename)
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

fn read_lines<P>(filename: P) -> Result<io::Lines<io::BufReader<File>>>
where
    P: AsRef<Path>,
{
    let file = File::open(filename)?;
    Ok(io::BufReader::new(file).lines())
}
