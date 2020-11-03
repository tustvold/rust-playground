use nom::IResult;
use nom::{
    branch::alt,
    bytes::complete::tag,
    character::complete::{char, digit1, space0},
    combinator::{cut, map, map_res, opt, recognize},
    multi::fold_many0,
    number::complete::float,
    sequence::{delimited, pair, preceded, separated_pair},
};

use calculator_client::{ComputeOperation, ComputeValue};

use super::Expr;

fn parse_constant(i: &str) -> IResult<&str, ComputeValue> {
    let decimal = separated_pair(pair(opt(char('-')), digit1), char('.'), opt(digit1));

    preceded(
        space0,
        alt((
            map_res(recognize(decimal), |digit_str: &str| {
                digit_str.parse().map(ComputeValue::Float)
            }),
            map_res(digit1, |digit_str: &str| {
                digit_str.parse().map(ComputeValue::Int)
            }),
            map_res(preceded(tag("-"), digit1), |digit_str: &str| {
                digit_str.parse().map(|x: i32| ComputeValue::Int(-x))
            }),
            map(float, ComputeValue::Float),
        )),
    )(i)
}

fn parse_multiply(i: &str) -> IResult<&str, Expr> {
    let enclosed_expression = preceded(
        space0,
        delimited(char('('), parse_expression, cut(char(')'))),
    );

    let inner = alt((map(parse_constant, Expr::Constant), enclosed_expression));

    let (i, init) = inner(i)?;

    fold_many0(
        preceded(space0, pair(alt((char('*'), char('/'))), cut(inner))),
        init,
        |l, (op, r)| {
            if op == '*' {
                Expr::Application(ComputeOperation::Mul, Box::new(l), Box::new(r))
            } else {
                Expr::Application(ComputeOperation::Div, Box::new(l), Box::new(r))
            }
        },
    )(i)
}

fn parse_expression(i: &str) -> IResult<&str, Expr> {
    let (i, init) = parse_multiply(i)?;

    fold_many0(
        preceded(
            space0,
            pair(alt((char('+'), char('-'))), cut(parse_multiply)),
        ),
        init,
        |l, (op, r)| {
            if op == '+' {
                Expr::Application(ComputeOperation::Add, Box::new(l), Box::new(r))
            } else {
                Expr::Application(ComputeOperation::Sub, Box::new(l), Box::new(r))
            }
        },
    )(i)
}

#[derive(Debug, Clone)]
pub struct ParseError(pub String);

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ParseError: {}", self.0)
    }
}

impl std::error::Error for ParseError {}

pub fn parse(i: &str) -> Result<Expr, ParseError> {
    match parse_expression(i) {
        Ok((remaining, r)) => {
            if remaining != "" {
                return Err(ParseError(format!("Unexpected token at \"{}\"", remaining)));
            }
            Ok(r)
        }
        Err(nom::Err::Error((i, _))) | Err(nom::Err::Failure((i, _))) => {
            Err(ParseError(format!("Unexpected token at \"{}\"", i)))
        }
        Err(_) => Err(ParseError("Parse Error".to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(e: &Expr) -> ComputeValue {
        match e {
            Expr::Constant(v) => *v,
            Expr::Application(op, l, r) => match op {
                ComputeOperation::Add => eval(l) + eval(r),
                ComputeOperation::Sub => eval(l) - eval(r),
                ComputeOperation::Mul => eval(l) * eval(r),
                ComputeOperation::Div => eval(l) / eval(r),
            },
        }
    }

    #[test]
    fn test_parse_constant() -> Result<(), Box<dyn std::error::Error>> {
        let (r1, v1) = parse_constant("442")?;
        let (r2, v2) = parse_constant("-34")?;
        let (r3, v3) = parse_constant("442.78")?;
        let (r4, v4) = parse_constant("-33.12")?;

        assert_eq!(r1, "");
        assert_eq!(r2, "");
        assert_eq!(r3, "");
        assert_eq!(r4, "");
        assert_eq!(v1, ComputeValue::Int(442));
        assert_eq!(v2, ComputeValue::Int(-34));
        assert_eq!(v3, ComputeValue::Float(442.78));
        assert_eq!(v4, ComputeValue::Float(-33.12));
        Ok(())
    }

    #[test]
    fn test_parse_expression() -> Result<(), Box<dyn std::error::Error>> {
        let (r1, v1) = parse_expression("332+23.0- 15")?;

        let evaluated = eval(&v1);

        assert_eq!(evaluated, ComputeValue::Float(340.0));
        assert_eq!(r1, "");
        match v1 {
            Expr::Application(ComputeOperation::Sub, l, r) => {
                match *l {
                    Expr::Application(ComputeOperation::Add, l, r) => {
                        assert_eq!(*l, Expr::Constant(ComputeValue::Int(332)));
                        assert_eq!(*r, Expr::Constant(ComputeValue::Float(23.0)));
                    }
                    _ => panic!("{:?} doesn't match", r),
                }
                assert_eq!(*r, Expr::Constant(ComputeValue::Int(15)));
            }
            _ => panic!("{:?} doesn't match", v1),
        }
        Ok(())
    }

    #[test]
    fn test_eval() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(eval(&parse_expression("34/2")?.1), ComputeValue::Int(17));
        assert_eq!(
            eval(&parse_expression("34 +6/ 2")?.1),
            ComputeValue::Int(37)
        );
        assert_eq!(
            eval(&parse_expression("(34 +6)/ 2")?.1),
            ComputeValue::Int(20)
        );
        assert_eq!(
            eval(&parse_expression("3 * 4 / (6+54.) * 5 - 1")?.1),
            ComputeValue::Float(0.0)
        );
        Ok(())
    }

    #[test]
    fn test_parse() -> Result<(), Box<dyn std::error::Error>> {
        let r1 = parse("34 +f6/ 2").unwrap_err();
        let r2 = parse("34a +f6/ 2").unwrap_err();

        assert_eq!(r1.0, "Unexpected token at \"f6/ 2\"".to_string());
        assert_eq!(r2.0, "Unexpected token at \"a +f6/ 2\"".to_string());

        Ok(())
    }
}
