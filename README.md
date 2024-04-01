# breweri

### Simple TUI frontend for [Homebrew](https://brew.sh).

Breweri is a fork of [parui](https://github.com/Vonr/parui).

Requires curl and jq.

```sh
brew install curl jq
```

### Usage

```
Usage: breweri [OPTION]... QUERY
        Search for QUERY in Homebrew repositories,
        Example:
           breweri rustup

        Options:
           -h
               Print this help and exit
```

### Keybinds

breweri adopts vim-like keybinds.

| Key                    | Mode   | Action                    |
|------------------------|--------|---------------------------|
| \<Return\>             | Insert | Search for query          |
| \<C-w\>                | Insert | Removes previous word     |
| \<C-c\>                | Both   | Exits breweri             |
| \<Escape\>             | Both   | Switch Modes              |
| i, /                   | Select | Enter Insert Mode         |
| \<Return\>             | Select | Install selected packages |
| \<C-j\>, \<C-Down\>    | Select | Moves info one row down   |
| \<C-k\>, \<C-Up\>      | Select | Moves info one row up     |
| h, \<Left\>, \<PgUp\>  | Select | Moves one page back       |
| j, \<Down\>            | Select | Moves one row down        |
| k, \<Up\>              | Select | Moves one row up          |
| l, \<Right\>, \<PgDn\> | Select | Moves one page forwards   |
| g, \<Home\>            | Select | Go to start               |
| G, \<End\>             | Select | Go to end                 |
| \<Space\>              | Select | Select/deselect package   |
| c                      | Select | Clear selections          |
| \<S-R\>                | Select | Remove selected packages  |
| q                      | Select | Exits breweri             |
