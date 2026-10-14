# Brief: `parse_nat`

> `parse_nat(text)` reads a natural number written in decimal digits.
> It answers `Ok(n)` for text that is one or more digits and nothing
> else, and `Err(reason)` for anything else. The signature is

```
pub fn parse_nat[&r](text: &r [byte]) -> [] result.Result[int, &static [byte]]
```
