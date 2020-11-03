use std::ops::{Add, Div, Mul, Sub};

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ComputeRequest {
    pub operation: ComputeOperation,
    pub left: ComputeValue,
    pub right: ComputeValue,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ComputeOperation {
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "type", content = "value")]
pub enum ComputeValue {
    Int(i32),
    Float(f32),
}

impl ComputeValue {
    fn as_float(self) -> f32 {
        match self {
            Self::Int(i) => i as f32,
            Self::Float(f) => f,
        }
    }
}

macro_rules! op {
    ( $t: ty, $f: ident ) => {
        impl $t for ComputeValue {
            type Output = ComputeValue;

            fn $f(self, rhs: Self) -> Self::Output {
                match (self, rhs) {
                    (Self::Int(l), Self::Int(r)) => Self::Int(l.$f(r)),
                    (l, r) => Self::Float(l.as_float().$f(r.as_float())),
                }
            }
        }
    };
}

op!(Add, add);
op!(Sub, sub);
op!(Mul, mul);
op!(Div, div);
