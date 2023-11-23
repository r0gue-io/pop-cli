# DoTemplate
<img src="https://private-user-images.githubusercontent.com/15669111/285133483-af63fbad-6eb3-434b-a1b6-78f15f0b4a2a.jpeg?jwt=eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJnaXRodWIuY29tIiwiYXVkIjoicmF3LmdpdGh1YnVzZXJjb250ZW50LmNvbSIsImtleSI6ImtleTEiLCJleHAiOjE3MDA3Mjc2NDAsIm5iZiI6MTcwMDcyNzM0MCwicGF0aCI6Ii8xNTY2OTExMS8yODUxMzM0ODMtYWY2M2ZiYWQtNmViMy00MzRiLWExYjYtNzhmMTVmMGI0YTJhLmpwZWc_WC1BbXotQWxnb3JpdGhtPUFXUzQtSE1BQy1TSEEyNTYmWC1BbXotQ3JlZGVudGlhbD1BS0lBSVdOSllBWDRDU1ZFSDUzQSUyRjIwMjMxMTIzJTJGdXMtZWFzdC0xJTJGczMlMkZhd3M0X3JlcXVlc3QmWC1BbXotRGF0ZT0yMDIzMTEyM1QwODE1NDBaJlgtQW16LUV4cGlyZXM9MzAwJlgtQW16LVNpZ25hdHVyZT0xMmZmMGU0ZmI4NDZjM2NkM2M5YmE3ZmRkZTlkZWUwYzhmMjc0ZDA1ZWM0NDUxOWM5NmE5MTg1ZDJhOTU4OGNkJlgtQW16LVNpZ25lZEhlYWRlcnM9aG9zdCZhY3Rvcl9pZD0wJmtleV9pZD0wJnJlcG9faWQ9MCJ9._lY6Hm8ZSFmk-SHbpT8ycw8SkRDjeu3g72f21z4DAuQ" height=400px width=400px  align="left"></img>

Your one-stop entry into the exciting world of Blockchain development with *Polkadot*

## Getting Started

Use `DoTemplate` to either clone of your existing templates or instantiate a new parachain template: 

```sh
# Create a minimal parachain template
dotemplate create my-app
# Get the extended-parachain-template
dotemplate create my-app ept
# Get a pallet-contracts enabled template
dotemplate create my-app cpt
# Get a evm compatible parachain template
dotemplate create my-app fpt
```

You can also customize a template by providing config options for token symbol (as it appears on polkadot-js apps UI), token decimals, and the initial endowment for substrate developer accounts. Here's how: 

```sh
# Create a minimal parachain template
dotemplate create my-app --symbol DOT --decimals 6 --endowment 1_000_000_000
```
There's also the shorter version: 
```sh
dotemplate create my-app -s DOT -d 6 -i 1_000_000_000
```
