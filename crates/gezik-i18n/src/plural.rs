//! Plural rules (spec 15 §4.1): the built-in languages' as a table, and gettext's
//! `Plural-Forms` header (`nplurals=3; plural=(n==1 ? 0 : …);`) for dropped files, read into a
//! small expression tree: `n`, numbers, `?:`, `|| &&`, `== != < > <= >=`, `+ - * / %`, `!`,
//! parentheses. No loops; nesting and length are bounded, so any header is safe to evaluate.

/// Longest expression read (the longest real one, Arabic's, is ~90 characters).
const MAX_LEN: usize = 512;
/// Deepest nesting of parentheses, `!` and `?:`.
const MAX_DEPTH: usize = 64;

/// The built-in languages' rules (spec 15 §8.1 lists them; CLDR's integer cases).
pub fn builtin(code: &str) -> Option<&'static str> {
    Some(match code {
        "en" | "de" | "es" => "nplurals=2; plural=(n != 1);",
        "fr" => "nplurals=2; plural=(n > 1);",
        "tr" | "zh-Hans" => "nplurals=1; plural=0;",
        _ => return None,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Expr {
    N,
    Num(u64),
    Not(Box<Expr>),
    Bin(Op, Box<Expr>, Box<Expr>),
    If(Box<Expr>, Box<Expr>, Box<Expr>),
}

impl Expr {
    fn eval(&self, n: u64) -> u64 {
        match self {
            Expr::N => n,
            Expr::Num(v) => *v,
            Expr::Not(e) => u64::from(e.eval(n) == 0),
            Expr::If(c, a, b) => {
                if c.eval(n) != 0 {
                    a.eval(n)
                } else {
                    b.eval(n)
                }
            }
            Expr::Bin(op, a, b) => {
                let a = a.eval(n);
                // Short-circuit as C does.
                match op {
                    Op::Or if a != 0 => return 1,
                    Op::And if a == 0 => return 0,
                    _ => {}
                }
                let b = b.eval(n);
                match op {
                    Op::Or | Op::And => u64::from(b != 0),
                    Op::Eq => u64::from(a == b),
                    Op::Ne => u64::from(a != b),
                    Op::Lt => u64::from(a < b),
                    Op::Gt => u64::from(a > b),
                    Op::Le => u64::from(a <= b),
                    Op::Ge => u64::from(a >= b),
                    Op::Add => a.wrapping_add(b),
                    Op::Sub => a.wrapping_sub(b),
                    Op::Mul => a.wrapping_mul(b),
                    Op::Div => a.checked_div(b).unwrap_or(0),
                    Op::Rem => a.checked_rem(b).unwrap_or(0),
                }
            }
        }
    }
}

/// A language's plural rule: how many forms, and which one a number takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plural {
    pub count: usize,
    expr: Expr,
}

impl Default for Plural {
    /// English's: one, other.
    fn default() -> Self {
        Plural::parse(builtin("en").unwrap_or_default()).unwrap_or(Plural { count: 2, expr: Expr::N })
    }
}

impl Plural {
    /// Reads a `Plural-Forms` header value.
    pub fn parse(header: &str) -> Result<Plural, String> {
        let (mut count, mut expr) = (None, None);
        for part in header.split(';') {
            let Some((key, value)) = part.split_once('=') else { continue };
            match key.trim() {
                "nplurals" => {
                    let n = value.trim().parse::<usize>().ok().filter(|n| (1..=6).contains(n));
                    count = Some(n.ok_or_else(|| format!("nplurals must be 1 to 6, not \"{}\"", value.trim()))?);
                }
                "plural" => expr = Some(value.trim()),
                _ => {}
            }
        }
        let count = count.ok_or("Plural-Forms has no nplurals")?;
        let text = expr.ok_or("Plural-Forms has no plural")?;
        if text.len() > MAX_LEN {
            return Err(format!("the plural expression is longer than {MAX_LEN} characters"));
        }
        let tokens = tokenize(text)?;
        let mut parser = Parser { tokens: &tokens, at: 0, depth: 0 };
        let expr = parser.ternary()?;
        if parser.at != tokens.len() {
            return Err(format!("unexpected \"{}\" in the plural expression", tokens[parser.at].text()));
        }
        Ok(Plural { count, expr })
    }

    /// The form `n` takes; a rule that answers past the last form gets the first.
    pub fn index(&self, n: u64) -> usize {
        usize::try_from(self.expr.eval(n)).ok().filter(|&i| i < self.count).unwrap_or(0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Token {
    N,
    Num(u64),
    Op(&'static str),
}

impl Token {
    fn text(&self) -> String {
        match self {
            Token::N => "n".to_owned(),
            Token::Num(v) => v.to_string(),
            Token::Op(s) => (*s).to_owned(),
        }
    }
}

fn tokenize(text: &str) -> Result<Vec<Token>, String> {
    const OPS: [&str; 18] =
        ["||", "&&", "==", "!=", "<=", ">=", "<", ">", "+", "-", "*", "/", "%", "!", "?", ":", "(", ")"];
    let mut out = Vec::new();
    let mut rest = text.trim_start();
    while !rest.is_empty() {
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        if digits > 0 {
            let value = rest[..digits].parse().map_err(|_| format!("number too large: {}", &rest[..digits]))?;
            out.push(Token::Num(value));
            rest = &rest[digits..];
        } else if let Some(after) = rest.strip_prefix('n') {
            out.push(Token::N);
            rest = after;
        } else if let Some(op) = OPS.iter().find(|op| rest.starts_with(**op)) {
            out.push(Token::Op(op));
            rest = &rest[op.len()..];
        } else {
            let c = rest.chars().next().unwrap_or(' ');
            return Err(format!("unexpected \"{c}\" in the plural expression"));
        }
        rest = rest.trim_start();
    }
    Ok(out)
}

struct Parser<'a> {
    tokens: &'a [Token],
    at: usize,
    depth: usize,
}

const LEVELS: [&[(&str, Op)]; 6] = [
    &[("||", Op::Or)],
    &[("&&", Op::And)],
    &[("==", Op::Eq), ("!=", Op::Ne)],
    &[("<=", Op::Le), (">=", Op::Ge), ("<", Op::Lt), (">", Op::Gt)],
    &[("+", Op::Add), ("-", Op::Sub)],
    &[("*", Op::Mul), ("/", Op::Div), ("%", Op::Rem)],
];

impl Parser<'_> {
    fn peek(&self) -> Option<Token> {
        self.tokens.get(self.at).copied()
    }

    fn eat(&mut self, op: &'static str) -> bool {
        let found = self.peek() == Some(Token::Op(op));
        self.at += usize::from(found);
        found
    }

    fn deeper(&mut self) -> Result<(), String> {
        self.depth += 1;
        if self.depth > MAX_DEPTH { Err("the plural expression is nested too deep".to_owned()) } else { Ok(()) }
    }

    fn ternary(&mut self) -> Result<Expr, String> {
        self.deeper()?;
        let cond = self.binary(0)?;
        let out = if self.eat("?") {
            let a = self.ternary()?;
            if !self.eat(":") {
                return Err("\"?\" without \":\" in the plural expression".to_owned());
            }
            let b = self.ternary()?;
            Expr::If(Box::new(cond), Box::new(a), Box::new(b))
        } else {
            cond
        };
        self.depth -= 1;
        Ok(out)
    }

    fn binary(&mut self, level: usize) -> Result<Expr, String> {
        let Some(ops) = LEVELS.get(level) else { return self.unary() };
        let mut left = self.binary(level + 1)?;
        while let Some(Token::Op(text)) = self.peek() {
            let Some(&(_, op)) = ops.iter().find(|(t, _)| *t == text) else { break };
            self.at += 1;
            let right = self.binary(level + 1)?;
            left = Expr::Bin(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<Expr, String> {
        let token = self.peek().ok_or("the plural expression ends too soon")?;
        self.at += 1;
        match token {
            Token::N => Ok(Expr::N),
            Token::Num(v) => Ok(Expr::Num(v)),
            Token::Op("!") => {
                self.deeper()?;
                let inner = self.unary()?;
                self.depth -= 1;
                Ok(Expr::Not(Box::new(inner)))
            }
            Token::Op("(") => {
                let inner = self.ternary()?;
                if !self.eat(")") {
                    return Err("\"(\" without \")\" in the plural expression".to_owned());
                }
                Ok(inner)
            }
            other => Err(format!("unexpected \"{}\" in the plural expression", other.text())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(header: &str) -> Plural {
        Plural::parse(header).unwrap_or_else(|e| panic!("{header}: {e}"))
    }

    #[test]
    fn built_in_languages() {
        type Rule = fn(u64) -> usize;
        let rules: [(&str, Rule); 6] = [
            ("en", |n| usize::from(n != 1)),
            ("de", |n| usize::from(n != 1)),
            ("es", |n| usize::from(n != 1)),
            ("fr", |n| usize::from(n > 1)),
            ("tr", |_| 0),
            ("zh-Hans", |_| 0),
        ];
        for (code, expected) in rules {
            let plural = rule(builtin(code).unwrap());
            for n in 0..=1000 {
                assert_eq!(plural.index(n), expected(n), "{code} {n}");
            }
        }
        assert_eq!(rule(builtin("tr").unwrap()).count, 1);
        assert_eq!(builtin("pl"), None);
        assert_eq!(Plural::default(), rule(builtin("en").unwrap()));
    }

    #[test]
    fn dropped_files_rules() {
        let polish = rule("nplurals=3; plural=(n==1 ? 0 : n%10>=2 && n%10<=4 && (n%100<10 || n%100>=20) ? 1 : 2);");
        let russian = rule(
            "nplurals=3; plural=(n%10==1 && n%100!=11 ? 0 : n%10>=2 && n%10<=4 && (n%100<10 || n%100>=20) ? 1 : 2);",
        );
        let arabic = rule(
            "nplurals=6; plural=(n==0 ? 0 : n==1 ? 1 : n==2 ? 2 : n%100>=3 && n%100<=10 ? 3 : n%100>=11 ? 4 : 5);",
        );
        let few = |n: u64| (2..=4).contains(&(n % 10)) && !(12..=14).contains(&(n % 100));
        for n in 0..=1000u64 {
            let pl = if n == 1 {
                0
            } else if few(n) {
                1
            } else {
                2
            };
            assert_eq!(polish.index(n), pl, "pl {n}");
            let ru = if n % 10 == 1 && n % 100 != 11 {
                0
            } else if few(n) {
                1
            } else {
                2
            };
            assert_eq!(russian.index(n), ru, "ru {n}");
            let ar = match (n, n % 100) {
                (0, _) => 0,
                (1, _) => 1,
                (2, _) => 2,
                (_, 3..=10) => 3,
                (_, 11..=99) => 4,
                _ => 5,
            };
            assert_eq!(arabic.index(n), ar, "ar {n}");
        }
        assert_eq!(arabic.count, 6);
        // Arithmetic, `!`, precedence, division by zero, an answer past the last form.
        assert_eq!(rule("nplurals=3; plural=n*2+1-1/1%5;").index(1), 2);
        assert_eq!(rule("nplurals=2; plural=!(n == 1);").index(1), 0);
        assert_eq!(rule("nplurals=2; plural=n/0 + n%0;").index(7), 0);
        assert_eq!(rule("nplurals=2; plural=n;").index(5), 0);
        assert_eq!(rule(" plural = n != 1 ; nplurals = 2 ").index(2), 1);
    }

    #[test]
    fn broken_rules_are_refused() {
        for bad in [
            "",
            "nplurals=2;",
            "plural=n;",
            "nplurals=0; plural=0;",
            "nplurals=9; plural=0;",
            "nplurals=x; plural=0;",
            "nplurals=2; plural=;",
            "nplurals=2; plural=(n;",
            "nplurals=2; plural=n ? 1;",
            "nplurals=2; plural=n = 1;",
            "nplurals=2; plural=n 1;",
            "nplurals=2; plural=x;",
            "nplurals=2; plural=99999999999999999999999;",
            "nplurals=2; plural=n & 1;",
        ] {
            assert!(Plural::parse(bad).is_err(), "{bad}");
        }
        let deep = format!("nplurals=2; plural={}n{};", "(".repeat(100), ")".repeat(100));
        assert!(Plural::parse(&deep).unwrap_err().contains("deep"));
        let nots = format!("nplurals=2; plural={}n;", "!".repeat(100));
        assert!(Plural::parse(&nots).unwrap_err().contains("deep"));
        let long = format!("nplurals=2; plural=n{};", "+n".repeat(300));
        assert!(Plural::parse(&long).unwrap_err().contains("longer"));
    }
}
