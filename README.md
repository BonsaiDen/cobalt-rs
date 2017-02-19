# cobalt [![Build Status](https://img.shields.io/travis/BonsaiDen/cobalt-rs/master.svg?style=flat-square)](https://travis-ci.org/BonsaiDen/cobalt-rs) [![Build status](https://img.shields.io/appveyor/ci/BonsaiDen/cobalt-rs/master.svg?style=flat-square)](https://ci.appveyor.com/project/BonsaiDen/cobalt-rs) [![Crates.io](https://img.shields.io/crates/v/cobalt.svg?style=flat-square)](https://crates.io/crates/cobalt) [![License](https://img.shields.io/crates/l/cobalt.svg?style=flat-square)]() [![Coverage Status](https://coveralls.io/repos/github/BonsaiDen/cobalt-rs/badge.svg?branch=master)](https://coveralls.io/github/BonsaiDen/cobalt-rs?branch=master)

A [rust](https://rust-lang.org/) based networking library providing [virtual 
connections over UDP](http://gafferongames.com/networking-for-game-programmers/udp-vs-tcp/) 
with an included message layer supporting both unreliable messaging and reliable 
messages with optional in-order delivery. 

- [Documentation](https://bonsaiden.github.io/cobalt-rs/cobalt/index.html) for the latest master build.


## Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
cobalt = "0.22.0"
```

and this to your crate root:

```rust
extern crate cobalt;
```

For usage examples please refer to the documentation of the libraries server 
and client abstractions.

## License

Licensed under either of
 * Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)
at your option.


### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you shall be dual licensed as above, without any
additional terms or conditions.

