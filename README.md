# archive-list

> keeping tabs on lots of repos

`archive-list` is a tool to help keep track of a very large list of repositories.

it looks for a file `archlist` which is a list of urls. it checks the current working directory, then ~/.config/archlist and ~/.config/archlist/archlist for files, using the first found.

# subcommands

| command | description |
| `readme-get` | download readmes every repo in archlist |
| `add-org` | add all projects from an org into archlist |
| `read-history` | read browser history to get a list of candidates to add |

## readme-get

downloads README files from every repository in the archlist file. writes into cwd, uses a directory structure with the hostnaame as a component, so for example `./github.com/rektide/archlist/README.md`. processes from bottom of file to top. if the repo exists, directory will be created, even if README fails.

### processing

- by default, reads from the bottom up, to allow new urls to be added at top
- has `--top-down` / `-T` mode to go top down, new urls at end
- has `--refresh` / `-r` refresh mode to freshen README's but usually skips
- uses 4k-aligned buffer reading
- tracks position as line number, as lines read
- position stored in config file, updated async every 2s

### rate limiting

- designed to download at better than average rate, then pause
- uses `governor` crate for rate limiting
- operates at 1.5x velocity of specified rate with 2s time-base
- capped by check of provider-specific rate limits via `is_ok()`

### providers

supports multiple providers:

- github
- gitlab
- huggingface
- codeberg

provider detection from first url then a fetch-and-analyze module, provider-specific rate limits respected.

### failures

failures logged to `.fail` file with format: `<URL> <ERROR-CODE>`

error codes: NO-README, NO-REPO, INVALID-PROVIDER

### directories

creates directories for repos without README to mark attempted access
