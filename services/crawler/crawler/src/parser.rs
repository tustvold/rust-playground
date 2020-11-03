use html5ever::local_name;
use html5ever::tendril::*;
use html5ever::tokenizer::TagKind::StartTag;
use html5ever::tokenizer::{BufferQueue, Token, TokenSink, TokenSinkResult, Tokenizer};
use reqwest::Url;
use std::collections::HashSet;

pub(crate) struct Parser {
    tokenizer: Tokenizer<Sink>,
    queue: BufferQueue,
}

impl Parser {
    pub(crate) fn new(base: Url) -> Parser {
        let sink: Sink = Sink::new(base);
        let tokenizer = Tokenizer::new(sink, Default::default());
        let queue = BufferQueue::new();
        Parser { tokenizer, queue }
    }

    pub(crate) fn feed(&mut self, decoded: &str) {
        self.queue.push_back(StrTendril::from_slice(decoded));
        let _ = self.tokenizer.feed(&mut self.queue);
        assert!(self.queue.is_empty());
    }

    pub(crate) fn finalize(mut self) -> HashSet<Url> {
        self.tokenizer.end();
        self.tokenizer.sink.links
    }
}

pub struct Sink {
    base: Url,
    links: HashSet<Url>,
}

impl Sink {
    fn new(base: Url) -> Sink {
        Sink {
            base,
            links: Default::default(),
        }
    }
}

impl TokenSink for Sink {
    type Handle = ();

    fn process_token(&mut self, token: Token, _line_number: u64) -> TokenSinkResult<()> {
        if let Token::TagToken(tag) = token {
            if tag.kind == StartTag && tag.name == local_name!("a") {
                let value = tag
                    .attrs
                    .into_iter()
                    .find(|x| x.name.local == local_name!("href"))
                    .map(|x| x.value.to_string());

                if let Some(link) = value {
                    match self.base.join(&link) {
                        Ok(v) => {
                            self.links.insert(v);
                        }
                        Err(e) => {
                            println!("Invalid href: {}", e);
                        }
                    }
                }
            }
        }
        TokenSinkResult::Continue
    }
}
