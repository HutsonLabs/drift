# IronRDP patches

Drift's changes to the vendored IronRDP (`third_party/ironrdp`, upstream rev `b149f50`), one
`git format-patch` file per change, in apply order. They are the upstreaming queue: each applies
with `git am` onto upstream `b149f500b85124c513646494335fb6cee525d897`.

Export a patch after committing a change that touches only `third_party/ironrdp/`:

```sh
git format-patch -1 <sha> --relative=third_party/ironrdp -o third_party/ironrdp-patches/
```

See `docs/adr/M0-2-ironrdp-vendored-fork.md`. Do not open upstream PRs without the owner's consent.
