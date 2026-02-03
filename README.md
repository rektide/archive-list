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

- uses `reqgov` library for intelligent HTTP API rate limiting
- OriginRegistry provides per-origin rate limiting with automatic quota detection
- Smoother prevents micro-bursts with 2-second intervals at 1.5x velocity
- ConcurrencyRateLimiter controls concurrent requests (10 global, 2 per domain)
- ResponseAdapter auto-detects rate limit headers and configures limiters
- processes URLs concurrently with `buffer_unordered(10)` for backpressure control

**Automatic rate limit detection:**
Rate limits are automatically detected from response headers:

GitHub style:
- `x-ratelimit-remaining`: requests remaining in current window
- `x-ratelimit-limit`: total requests allowed per window
- `x-ratelimit-reset`: unix timestamp when limit resets

GitLab style:
- `ratelimit-remaining`: requests remaining
- `ratelimit-limit`: total requests allowed
- `ratelimit-reset`: unix timestamp when limit resets

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
