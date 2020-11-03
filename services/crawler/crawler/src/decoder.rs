use crate::crawler::CrawlError;
use encoding_rs::*;
use mime::Mime;
use reqwest::Response;

fn get_encoding(res: &Response) -> Result<&'static Encoding, CrawlError> {
    let content_type: Option<Mime> = res
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<Mime>().ok());

    if let Some(m) = content_type {
        if m.type_() != mime::TEXT || m.subtype() != mime::HTML {
            return Err(CrawlError::NonHtmlContent);
        }

        if let Some(charset) = m.get_param("charset") {
            if let Some(e) = Encoding::for_label(charset.as_str().as_bytes()) {
                return Ok(e);
            }
        }
    }
    Ok(UTF_8)
}

pub(crate) async fn streaming_decode(
    res: &mut Response,
    mut flush: impl FnMut(&str),
) -> Result<(), CrawlError> {
    let encoding = get_encoding(res)?;

    let mut decoder = encoding.new_decoder();
    let mut bytes_in_buffer = 0usize;
    let mut buffer_bytes = [0u8; 2048];
    let buffer: &mut str = std::str::from_utf8_mut(&mut buffer_bytes[..]).unwrap();

    while let Some(req_chunk) = res.chunk().await? {
        let mut total_read_from_current_input = 0usize;

        loop {
            let (result, read, written, had_errors) = decoder.decode_to_str(
                &req_chunk[total_read_from_current_input..],
                &mut buffer[bytes_in_buffer..],
                false,
            );
            if had_errors {
                return Err(CrawlError::DecodeError);
            }
            total_read_from_current_input += read;
            bytes_in_buffer += written;
            match result {
                CoderResult::InputEmpty => {
                    break;
                }
                CoderResult::OutputFull => {
                    flush(&mut buffer[..bytes_in_buffer]);
                    bytes_in_buffer = 0usize;
                    continue;
                }
            }
        }
    }

    // EOF
    loop {
        let (result, _, written, had_errors) =
            decoder.decode_to_str(b"", &mut buffer[bytes_in_buffer..], true);
        if had_errors {
            return Err(CrawlError::DecodeError);
        }
        bytes_in_buffer += written;

        flush(&buffer[..bytes_in_buffer]);
        bytes_in_buffer = 0usize;
        match result {
            CoderResult::InputEmpty => {
                break;
            }
            CoderResult::OutputFull => {
                continue;
            }
        }
    }
    Ok(())
}
