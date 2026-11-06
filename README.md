# LGTM.md

**Looks good to ReadMe.** Animated pixel-art widgets for your GitHub profile README, drawn every night from your real activity.

<img src="examples/preview/grass.gif" alt="Night field: one tuft of grass per day over the last 90 days, flowers for languages, fireflies for merged pull requests" width="100%">

A GitHub Action collects your contributions, languages and merged pull requests, then draws each widget as a GIF and commits it to your profile repository. One prebuilt binary, no server, no third-party service: your data never leaves GitHub.

Every widget answers one question in two seconds. The animation is there to show the number, not to decorate it.

## Quick start

You need a [profile repository](https://docs.github.com/en/account-and-profile/setting-up-and-managing-your-github-profile/customizing-your-profile/managing-your-profile-readme): a public repository named after your login (`<login>/<login>`).

**1. Add the workflow.** In your profile repository, create `.github/workflows/lgtm-md.yml`:

```yaml
name: LGTM.md

on:
  schedule:
    - cron: "30 0 * * *"   # every night, just after midnight UTC
  workflow_dispatch:

permissions:
  contents: write

concurrency: lgtm-md

jobs:
  draw:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: eliorpom-cmd/lgtm.md@v0.1.0
        with:
          token: ${{ secrets.LGTM_MD_TOKEN || github.token }}
          widgets: |
            flame
            grass
            records
      - name: Commit the widgets if they changed
        run: |
          git add widgets
          git diff --cached --quiet && exit 0
          git config user.name "github-actions[bot]"
          git config user.email "41898282+github-actions[bot]@users.noreply.github.com"
          git commit -m "widgets: draw the day's widgets"
          git push
```

**2. Run it once.** Open the **Actions** tab of your profile repository, pick **LGTM.md**, then **Run workflow**. About a minute later, the GIFs are in the `widgets/` folder.

**3. Show them in your README.** Paste one line per widget:

```html
<a href="https://github.com/eliorpom-cmd/lgtm.md"><img src="widgets/grass.gif" alt="My last 90 days on GitHub" width="100%"></a>
```

The image links back here so visitors can find the project. A small credit line under your widgets helps too:

```html
<sub>Made with <a href="https://github.com/eliorpom-cmd/lgtm.md">LGTM.md</a></sub>
```

From then on, the widgets redraw themselves every night.

## Count your private activity (optional)

With the default token, the widgets only see public activity. To count your work in private repositories too, give the Action a read-only token:

1. Open this pre-filled link: [create a fine-grained token](https://github.com/settings/personal-access-tokens/new?name=LGTM.md&description=Read-only+token+for+LGTM.md&expires_in=366&contents=read&pull_requests=read). It asks for **Contents** and **Pull requests**, both read-only.
2. Under **Repository access**, choose **All repositories**. The link cannot set this one for you.
3. Click **Generate token** and copy it.
4. In your profile repository, go to **Settings → Secrets and variables → Actions → New repository secret**. Name it `LGTM_MD_TOKEN` and paste the token.

The workflow above already uses `LGTM_MD_TOKEN` when it exists. The token can only read: it cannot push, open issues or change settings.

One limit comes from GitHub: a fine-grained token covers the repositories of a single owner, so pull requests in **another organization's private repositories** are not counted. Everything else is: private commits, private repositories' languages and their history.

The token expires after a year at most. GitHub emails you before it does; create a new one and replace the secret.

## Widgets

List the ones you want in the `widgets` input, one per line. Leave the input out to draw all of them except the duels.

| Widget | What it shows |
| --- | --- |
| `flame` | Your current streak of active days. The fire grows at 7, 30 and 100 days. |
| `counter` | Contributions over the last 12 months, rolling up from yesterday's count. |
| `hourglass` | Commits of the past year as coloured sand, one colour per language. Plays once, then the pile stays. |
| `ecg` | The last 7 days as a heartbeat, with the week's total and daily average. |
| `departures` | A split-flap board cycling through four of your stats. |
| `donut` | Your top languages, as the icing of a doughnut. |
| `grass` | One tuft of grass per day over 90 days. Flowers are languages, fireflies are merged pull requests. |
| `cherry-tree` | One blossom cluster per day over 90 days, one falling petal per merged pull request. |
| `forest` | One tree per week over 12 months, its height following that week's contributions. |
| `fireworks` | One rocket per pull request merged in the last 7 days. |
| `fireworks-commits` | One rocket per batch of commits in the last 7 days, coloured by language. |
| `records` | Best day, longest streak and longest break of the last 12 months. |
| `median` | Your contributions against the median of the accounts you follow. |
| `duel-<login>` | You against a friend over 30 days. Add `-7` or `-365` for a week or a year. |

Every GIF carries a thin band underneath that says what it shows and when it was last updated.

### `flame` · `counter`

<img src="examples/preview/flame.gif" alt="Campfire for a 27-day streak" width="49%"> <img src="examples/preview/counter.gif" alt="Drum counter showing 2491 contributions" width="49%">

### `hourglass`

<img src="examples/preview/hourglass.gif" alt="Hourglass of coloured sand, one grain per 20 commits" width="100%">

### `ecg`

<img src="examples/preview/ecg.gif" alt="Heartbeat of the last 7 days on a green monitor" width="100%">

### `departures`

<img src="examples/preview/departures.gif" alt="Split-flap board showing 2491 contributions" width="100%">

### `donut`

<img src="examples/preview/donut.gif" alt="Doughnut with one icing colour per language" width="80%">

### `cherry-tree`

<img src="examples/preview/cherry-tree.gif" alt="Cherry tree in bloom at dusk, petals falling for merged pull requests" width="100%">

### `forest`

<img src="examples/preview/forest.gif" alt="Forest with one tree per week of the year" width="100%">

### `fireworks` · `fireworks-commits`

<img src="examples/preview/fireworks.gif" alt="Fireworks over a lake, one rocket per merged pull request" width="100%">

<img src="examples/preview/fireworks-commits.gif" alt="Fireworks over a lake, one rocket per batch of commits" width="100%">

### `records`

<img src="examples/preview/records.gif" alt="Three brass plaques: best day, longest streak, longest break" width="100%">

### `median`

<img src="examples/preview/median.gif" alt="Two brick towers: your contributions and the median of the accounts you follow" width="70%">

### `duel-<login>`

<img src="examples/preview/duel-rival.gif" alt="Duel over 30 days between two accounts" width="100%">

The friend's side only uses public data. Duel as many friends as you like: `duel-torvalds`, `duel-octocat-7`… For a login that itself ends in `-7` or `-365`, write the period explicitly: `duel-<login>-30`.

## Action inputs

| Input | Default | What it does |
| --- | --- | --- |
| `token` | `github.token` | Token used to read your activity. See [private activity](#count-your-private-activity-optional). |
| `user` | the repository owner | GitHub login to draw. |
| `widgets` | every widget except duels | Widget names, separated by spaces or new lines. |
| `output-dir` | `widgets` | Folder where the GIFs are written. |

Pin a release tag such as `@v0.1.0` rather than a branch: a moving reference runs whatever code it points to, with your token.

## Run it on your computer

```sh
cargo build --release
./target/release/lgtm-md collect <login> data.json      # reads your activity with `gh auth token` or GITHUB_TOKEN
./target/release/lgtm-md data.json flame grass --out widgets
```

Without widget names, every widget is drawn. `examples/data.json` holds sample data for a made-up account, so you can try the drawing without a token.

## What gets counted

- **Contributions** are the numbers of your GitHub contribution graph: commits, pull requests, issues and reviews, private ones included with a token.
- **Commits by language** come from the history of the repositories you own, plus your contributions to other people's repositories. A commit takes the main language of its repository.
- **Languages** are the languages of your repositories, weighted by size. Markup (HTML, CSS, MDX, notebooks…) is left out of the doughnut and the flowers.
- **Days** follow UTC, like GitHub's contribution graph.

## Support

LGTM.md is free and MIT-licensed. If you like it, you can [buy me a coffee on Ko-fi](https://ko-fi.com/eliorpom).

## License

[MIT](LICENSE)
