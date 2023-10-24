# Pseudo mutex

(aka called clodotex - Geehaich)

It's a new asynchroneous mutex developped for the only purpose of [Ethrerage](https://github.com/jimy-byerley/etherage) project.
It's has been tested on asynchroneouse multithread context with the following result


Test between OS mutex and this library (debug - non optimize) for 50 task, 4 thread.

|              | Time   |
|--------------|--------|
| Pseudo-mutex | 94327  |
| OS mutex     | 94293  |
