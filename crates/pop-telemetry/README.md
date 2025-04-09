# Telemetry

Anonymous Usage Metrics Collection with Umami

## Umami

Umami is an analytics platform that is privacy-focused and open-source. It is a great alternative to Google Analytics.
We self-host Umami in the EU to safeguard all anonymous data.

You can read more about Umami [here](https://umami.is/).

## Why Collect Anonymous Usage Metrics?

We understand the importance of privacy and are committed to safeguarding the data of our users. Collecting
anonymous usage metrics can provide invaluable insights into how our CLI tool is being utilized, allowing us to improve
its performance, reliability, and user experience. Here's why we collect anonymous usage metrics:

1. **Improving User Experience**: By understanding how our CLI tool is used in real-world scenarios, we can identify
   areas for improvement and prioritize features that will enhance the overall user experience.

2. **Bug Identification and Resolution**: Anonymous usage metrics help us identify and prioritize bugs and issues that
   may not be immediately apparent. This allows us to provide quicker resolutions and ensure a smoother user experience.

3. **Feature Prioritization**: Knowing which features are used most frequently helps us prioritize development efforts
   and allocate resources effectively to meet the needs of our users.

## What We Collect

We do **not** collect **any** personal information. We do not collect file names, GitHub repositories, or anything
that is potentially sensitive. We do not even try to collect and sanitize this data, we simply do not collect it.
Here is what we do collect, anonymously:

1. **Command Usage**: We collect information about the commands that are executed using our CLI tool. This includes
   the type of command and options used. For example, we may report that `pop new parachain` was executed with the Pop
   Standard template.
2. **CLI Usage**: We collect information about how often the CLI tool is used.

## Our Commitment to Privacy

We take privacy seriously and are committed to protecting the anonymity of our users. Here's how we ensure your privacy:

- **Anonymous Data Collection**: We only collect anonymized usage metrics, which do not contain any personally
  identifiable information.

- **Transparency**: We are transparent about the data we collect and how it is used. Some portions of the data will be
  made public,
  such as the number of times a command was executed, and total number of users.

- **Privacy-First Platform**: We use Umami, a privacy-focused and open-source analytics platform, to collect anonymous
  usage metrics.

- **EU and GDPR Compliance**: We self-host Umami in the EU to ensure compliance with the General Data Protection
  Regulation (GDPR) and safeguard the data of our users. This ensures there is no 3rd party involved in the data.

## How to Opt-Out

If you prefer not to participate in anonymous usage metrics collection, there are a
few ways you can opt out. We support the [DO_NOT_TRACK](https://consoledonottrack.com/) and CI environment variable
standards.

1. Set the `DO_NOT_TRACK` environment variable to `true` or `1`
2. Set the `CI` environment variable to `true` or `1`
3. Completely disable telemetry, by installing with telemetry compiled out:

    ```bash
    cargo install --locked --no-default-features --features contract,parachain --git "https://github.com/r0gue-io/pop-cli"
    ```

## Questions or Concerns?

If you have any questions or concerns regarding our telemetry practices, please don't
hesitate to contact us:

- Contact form: [r0gue.io/contact](https://r0gue.io/contact)
- Telegram: [Pop](https://t.me/onpopio)

Thank you for your support and understanding.
