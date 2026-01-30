please review this project and get an understanding of it, especially how we are attempting to create rate limited connections to servers, using various tokens or default connections, and optionally using tokens.

we want to create a design document markdown file for a new library that does just this (rate-limited per origin, optional token provided) connection work.

the base implementation should probably be based around `reqwest-ratelimit`, which will require us to implement our own actual rate limit check, which will have to assess the origin and find rate limit.

assess

we need various detection strategies per origin, and to be able to detect which if any apply. keep checking for new rate limits if no rate limit strategy is identified, and use a default governor rate limit of 100r / hr.

please

i'd like to use reqwest middleware such as reqwest-ratelimit , reqwest-tracing in the reqwest-middleware monorepo. we defintely want to use governor crate. use context7 and deepwiki to resekkarch these crates.

---

please review @doc/EXPLORE-draft-httpapi-ratelimit-headers.md . come up with a doc/PLAN-httpapi.opus.md for an implementation, based off governors. instead of targetting our app for using it, target this design to work with reqwest-ratelimit. under the hood, use `governor` crate for rate-limiting, but i think we will need multiple. use deepwiki. use context7. you may need to lookup reqwest too to understand what reqwest-ratelimit passes us. create a clean separation barrier between a system that looks at domain name / origin & which has a registry of limiters, and the http api rate limiter. a core architectural feature is that there are rate limiters across multiple time bases in this spec. also specify a smoother, which divides the fastest time base (say 5 minutes) into a much smaller time base (2 seconds), such that requests are smoothed out across the fast duration. provide a "velocity" factor for the smoother so it could race to completion faster. add a section at the end that discusses how we can adapt the system here with it
