#![allow(clippy::type_complexity)]

use anyhow::anyhow;
use anyhow::Result;
use std::collections::HashMap;

use nom::{
    branch::alt,
    bytes::complete::take_until,
    character::complete::{char, digit1, multispace0},
    combinator::{map, map_res},
    multi::separated_list0,
    sequence::{delimited, preceded, tuple},
    IResult,
};

use crate::midi::MidiConfig;

/// Parses * or a u8 ranged number
fn parse_u8_or_star(input: &str) -> IResult<&str, u8> {
    let star_parser = map(char('*'), |_| 255_u8);
    let num_parser = map_res(preceded(multispace0, digit1), str::parse::<u8>);

    // Try parsing as a number first, and if it fails, try parsing as the '*' character.
    alt((num_parser, star_parser))(input)
}

/// Parses a u8 ranged number
fn parse_u8(input: &str) -> IResult<&str, u8> {
    map_res(preceded(multispace0, digit1), str::parse::<u8>)(input)
}

/// Parses the port name until the first [
fn parse_port_name(input: &str) -> IResult<&str, &str> {
    let (input, _) = multispace0(input)?; // Consume leading spaces
    let (input, name) = take_until("[")(input)?;
    let (name, _) = name.trim_end().split_at(name.trim_end().len()); // Trim trailing spaces in the port name
    Ok((input, name))
}

/// Parses channel and its CC numbers a three element tuple (<u8 or *>, u8, u8)
fn parse_channel_and_ccs(input: &str) -> IResult<&str, (u8, u8, u8)> {
    delimited(
        preceded(multispace0, char('(')),
        tuple((
            preceded(multispace0, parse_u8_or_star),
            preceded(
                multispace0,
                delimited(
                    preceded(multispace0, char(',')),
                    parse_u8,
                    preceded(multispace0, char(',')),
                ),
            ),
            preceded(multispace0, parse_u8),
        )),
        preceded(multispace0, char(')')),
    )(input)
}

/// Parse a list of channels and CCs [(..), (..), (..)]
fn parse_list(input: &str) -> IResult<&str, Vec<(u8, u8, u8)>> {
    delimited(
        preceded(multispace0, char('[')),
        separated_list0(preceded(multispace0, char(',')), parse_channel_and_ccs),
        preceded(multispace0, char(']')),
    )(input)
}

/// Parses an entire port configuration
fn parse_port(input: &str) -> IResult<&str, (&str, Vec<(u8, u8, u8)>)> {
    // Consume leading spaces
    let (input, _) = multispace0(input)?;

    // Parse port name
    let (input, port_name) = parse_port_name(input)?;

    // Consume characters until the next opening bracket `[`
    let (input, _) = take_until("[")(input)?;

    // Parse the list of channels and CCs
    let (input, channels_and_ccs) = parse_list(input)?;

    Ok((input, (port_name, channels_and_ccs)))
}

/// Parses the complete MIDI input or output configuration
fn parse_midi_config_raw(input: &str) -> IResult<&str, Vec<(&str, Vec<(u8, u8, u8)>)>> {
    delimited(
        preceded(multispace0, char('[')),
        separated_list0(preceded(multispace0, char(',')), parse_port),
        preceded(multispace0, char(']')),
    )(input)
}

/// Parses the [`MidiConfig`] from the provided configuration string.
pub fn parse_midi_config(input: &str) -> Result<MidiConfig> {
    let mut map: HashMap<String, Vec<(u8, u8, u8)>> = HashMap::new();
    let (_, port_configs) =
        parse_midi_config_raw(input).map_err(|_| anyhow!("Can not parse provided MIDI config."))?;
    for (name, channel_configs) in port_configs {
        map.insert(name.to_string(), channel_configs);
    }
    Ok(MidiConfig(map))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_u8() {
        assert_eq!(parse_u8("23"), Ok(("", 23)));
        assert_eq!(parse_u8("  23"), Ok(("", 23)));
        assert_eq!(parse_u8("0"), Ok(("", 0)));
        assert!(parse_u8("256").is_err());
    }

    #[test]
    fn test_parse_u8_or_star() {
        assert_eq!(parse_u8_or_star("23"), Ok(("", 23)));
        assert_eq!(parse_u8_or_star("  23"), Ok(("", 23)));
        assert_eq!(parse_u8_or_star("0"), Ok(("", 0)));
        assert_eq!(parse_u8_or_star("*"), Ok(("", 255)));
        assert!(parse_u8_or_star("256").is_err());
    }

    #[test]
    fn test_parse_port_name() {
        assert_eq!(parse_port_name("some port["), Ok(("[", "some port")));
        assert_eq!(
            parse_port_name("  spaced port  ["),
            Ok(("[", "spaced port"))
        );
    }

    #[test]
    fn test_parse_channel_and_ccs() {
        assert_eq!(parse_channel_and_ccs("(1,23,44)"), Ok(("", (1, 23, 44))));
        assert_eq!(
            parse_channel_and_ccs("(1 , 23 , 44)"),
            Ok(("", (1, 23, 44)))
        );
        assert_eq!(parse_channel_and_ccs(" ( 1 , 2 , 3 )"), Ok(("", (1, 2, 3))));
    }

    #[test]
    fn test_parse_port() {
        let expected = ("", ("some port", vec![(1, 23, 44), (12, 5, 6), (9, 0, 1)]));
        assert_eq!(
            parse_port("some port[(1,23,44), (12, 5, 6), (9, 0,1)]"),
            Ok(expected)
        );
    }

    #[test]
    fn test_parse_midi_config_raw() {
        let expected = Ok((
            "",
            vec![
                ("some port", vec![(1, 23, 44), (12, 5, 6), (9, 0, 1)]),
                ("another port", vec![(4, 55, 44)]),
                ("maybe another", vec![(2, 44, 33)]),
            ],
        ));

        assert_eq!(
            parse_midi_config_raw("[some port[(1,23,44), (12, 5, 6), (9, 0,1)], another port[(4,55, 44)],maybe another[(2,44,33)]]"),
            expected
        );

        // With more spaces
        let expected = Ok(("", vec![("a very spaced port", vec![(1, 2, 3)])]));

        assert_eq!(
            parse_midi_config_raw("[ a very spaced port  [ ( 1 , 2 , 3 ) ] ]"),
            expected
        );
    }

    #[test]
    fn test_parse_list() {
        let expected = Ok(("", vec![(1, 23, 44), (12, 5, 6), (9, 0, 1)]));
        assert_eq!(parse_list("[(1,23,44), (12, 5, 6), (9, 0,1)]"), expected);
    }

    #[test]
    fn test_trailing_and_leading_spaces() {
        let input = "[  spaced port   [ ( 1 , 2 , 3 ) ,  (4 ,5, 6) ] ]";
        let result = parse_midi_config_raw(input);
        assert_eq!(
            result,
            Ok(("", vec![("spaced port", vec![(1, 2, 3), (4, 5, 6)])]))
        );
    }

    #[test]
    fn test_special_chars_in_port_names() {
        let input = "[portname!@#[(1,2,3)]]";
        let result = parse_midi_config_raw(input);
        assert_eq!(result, Ok(("", vec![("portname!@#", vec![(1, 2, 3)])])));
    }

    #[test]
    fn test_star_in_tuple() {
        let input = "[port_name[(*,2,3), (4,5,6)]]";
        let result = parse_midi_config_raw(input);
        assert_eq!(
            result,
            Ok(("", vec![("port_name", vec![(255, 2, 3), (4, 5, 6)])]))
        );
    }
}
