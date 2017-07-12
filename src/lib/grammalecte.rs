// Copyright (C) 2017 Élisabeth HENRY.
//
// This file is part of Crowbook.
//
// Crowbook is free software: you can redistribute it and/or modify
// it under the terms of the GNU Lesser General Public License as published
// by the Free Software Foundation, either version 2.1 of the License, or
// (at your option) any later version.
//
// Crowbook is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Lesser General Public License for more details.
//
// You should have received a copy of the GNU Lesser General Public License
// along with Crowbook.  If not, see <http://www.gnu.org/licenses/>.

use rustc_serialize::json;
use reqwest;
use reqwest::Client;
use rayon::prelude::*;

use std::io::Read;

use text_view::view_as_text;
use text_view::insert_annotation;
use token::Token;
use token::Data;
use error::{Error, Result, Source};
use logger::Logger;

/// Represents a grammar error from Grammalecte
///
/// Note: lots of fields are missing
#[allow(non_snake_case)]
#[derive(RustcDecodable, RustcEncodable, Debug)]
struct GrammalecteError {
    pub sMessage: String,
    pub nStart: usize,
    pub nEnd: usize,
}

/// Contains a list of matches to errors
///
/// Corresponds to the JSON that LanguageTool-server sends back
///
/// Note: lots of fields are missing
#[allow(non_snake_case)]
#[derive(RustcDecodable, RustcEncodable, Debug)]
struct GrammalecteData {
    pub lGrammarErrors: Vec<GrammalecteError>,
}

#[derive(RustcDecodable, RustcEncodable, Debug)]
struct GrammalecteCheck {
    data: Vec<GrammalecteData>,
}
    
/// Grammalecte Checker
pub struct GrammalecteChecker {
    port: usize,
}

impl GrammalecteChecker {
    /// Initialize the grammarchecker
    pub fn new<S: Into<String>>(port: usize, lang: S) -> Result<GrammalecteChecker> {
        let lang = lang.into();
        if !lang.starts_with("fr") {
            return Err(Error::grammar_check(Source::empty(), lformat!("grammalecte only works with 'fr' lang")));
        }
        let checker = GrammalecteChecker {
            port: port,
        };

        let res = reqwest::get(&format!("http://localhost:{}/v2/languages", port))
            .map_err(|e| {
                Error::grammar_check(Source::empty(),
                                     lformat!("could not connect to grammalecte server: {error}",
                                              error = e))
            })?;
        if !res.status().is_success() {
            return Err(Error::grammar_check(Source::empty(),
                                            lformat!("server didn't respond with a OK status \
                                                      code")));
        }
        Ok(checker)
    }

    /// Send a query to LanguageTools server and get back a list of errors
    fn check(&self, text: &str) -> Result<GrammalecteCheck> {
        let params = [("text", text), ("options", "{\"apos\": false, \"nbsp\": false, \"esp\": false}")];

        let client = Client::new()
            .map_err(|e| {
                Error::grammar_check(Source::empty(),
                                     lformat!("could not start reqwest client: {error}",
                                              error = e))
            })?;


        let mut res = client.post(&format!("http://localhost:{}/v2/check", self.port))
            .map_err(|e| {
                Error::grammar_check(Source::empty(),
                                     lformat!("could not build post request: {error}",
                                              error = e))
            })?
            .form(&params)
            .map_err(|e| {
                Error::grammar_check(Source::empty(),
                                     lformat!("could not build form: {error}",
                                              error = e))
            })?
            .send()
            .map_err(|e| {
                Error::grammar_check(Source::empty(),
                                     lformat!("could not send request to server: {error}",
                                              error = e))
            })?;

        if !res.status().is_success() {
            return Err(Error::grammar_check(Source::empty(),
                                            lformat!("server didn't respond with a OK status \
                                                      code")));
        }

        let mut s = String::new();
        res.read_to_string(&mut s)
            .map_err(|e| {
                Error::grammar_check(Source::empty(),
                                     lformat!("could not read response: {error}", error = e))
            })?;
        let reponse: GrammalecteCheck = json::decode(&s)
            .map_err(|e| {
                Error::default(Source::empty(),
                               lformat!("could not decode JSON: {error}", error = e))
            })?;
        Ok(reponse)
    }
}


impl GrammalecteChecker {
    /// Check the grammar in a vector of tokens.
    ///
    /// This modifies the AST
    pub fn check_chapter(&self, tokens: &mut Vec<Token>) -> Result<()> {
        let res = tokens.par_iter_mut()
            .map(|token| {
                match *token {
                    Token::Paragraph(ref mut v) |
                    Token::Header(_, ref mut v) |
                    Token::BlockQuote(ref mut v) |
                    Token::List(ref mut v) |
                    Token::OrderedList(_, ref mut v) => {
                        let check = self.check(&view_as_text(v))?;
                        if check.data.len() >= 1 {
                            for error in &check.data[0].lGrammarErrors {
                                insert_annotation(v,
                                                  &Data::GrammarError(error.sMessage.clone()),
                                                  error.nStart,
                                                  error.nEnd - error.nStart);
                            }
                        }
                        if check.data.len() > 1 {
                            Logger::display_warning(lformat!("some error messages from Grammalecte were ignored because of format"));
                        }
                        Ok(())
                    },
                
                    _ => Ok(()),
                }
            })
            .find_any(|r| r.is_err());
        if let Some(err) = res {
            err
        } else {
            Ok(())
        }
    }
}
