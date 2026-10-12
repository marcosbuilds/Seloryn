# Validation and exhaustive errors — design example

**Not runnable.** Illustrates grammar beyond `if/elif` and typed failures.

```seloryn
enum SignupError:
    EmptyName
    InvalidEmail

record Signup:
    name: Text
    email: Text

fn validate(data: Signup) -> Result[Signup, SignupError]:
    require data.name.length > 0 else SignupError.EmptyName
    require data.email.contains("@") else SignupError.InvalidEmail
    return Ok(data)

fn message(data: Signup) -> Text:
    match validate(data):
        case Ok(valid):
            return "Hello, " + valid.name
        case Err(EmptyName):
            return "Name required"
        case Err(InvalidEmail):
            return "Email invalid"
```

Exhaustiveness, enum-qualified constructor syntax and pattern import rules need conformance tests before the syntax can be called stable.
