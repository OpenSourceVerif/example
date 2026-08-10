# terse-rust skill (ver1)

less boilerplate, no unnecessary ceremony.
pick one terminology for one thing. eliminate interchangeble terms, prefer no adjective.
use types in more structured ways, create less nominal type.
more `use`. resolve collision via alias.
- exception: `match` block uses `Enum::Variant`. `Variant => {}` has no syntatic indication： is `Variant` variant or variable binding?
use more traits for 
- ad-hoc polymorphism, e.g. overloading functions;
- abstraction;
- use less newtype for adapter pattern.