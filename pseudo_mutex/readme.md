# Magetex

(aka *clodotex* - Geehaich)

Just a library to support better asynchroneous mutex and lock guard mutex. Developped initialy only for the purpose of [Ethrerage](https://github.com/jimy-byerley/etherage) project.
It's has been tested on asynchroneouse multithread context with the following result

Test between OS mutex and this library (debug - non optimize) for 50 task, 4 thread.

| Mutex        | Time   |
|--------------|--------|
| Pseudo-mutex | 94327  |
| OS mutex     | 94293  |

Evolution:

- [X] Test and benchmark
- [X] Add documentation
- [ ] Add poison support
