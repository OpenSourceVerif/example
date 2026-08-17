# Style

ergonomic: less boilerplate, no unnecessary ceremony.
pick one terminology for one thing. eliminate interchangeble terms, prefer no adjective.
use types in more structured ways, create less nominal type.
more `use`. resolve collision via alias.
- exception: `match` block uses `Enum::Variant`. `Variant => {}` has no syntatic indication： is `Variant` variant or variable binding?
use more traits for 
- ad-hoc polymorphism, i.e. overloading;
- abstraction;
- use less newtype for adapter pattern.
no unnecessary function forwarding e.g. forward trait impl to inherent method.