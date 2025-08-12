# Disabling GitHub Copilot Auto-Reviews

To prefer CodeRabbit for code review and disable automatic GitHub Copilot reviews, follow these steps:

## Repository Settings

1. **Navigate to Repository Settings**:

   - Go to your repository on GitHub
   - Click on the "Settings" tab
   - Scroll down to "Code security and analysis"

1. **Disable Copilot Auto-Reviews** (if configured):

   - Look for "GitHub Copilot" or "Code review" settings
   - Disable automatic code review features
   - Turn off auto-generated review comments

## CodeRabbit Configuration

1. **Enable CodeRabbit**:

   - Visit [CodeRabbit Marketplace](https://github.com/marketplace/coderabbitai)
   - Install CodeRabbit AI for your repository
   - Grant necessary permissions

1. **Configure CodeRabbit Settings**:

   - Go to [CodeRabbit Dashboard](https://coderabbit.ai)
   - Configure review preferences
   - Set up integration with your repository

## Branch Protection Rules

Configure branch protection to require CodeRabbit reviews:

1. **Navigate to Branch Protection**:

   - Repository Settings → Branches
   - Add or edit rules for `main` and `develop` branches

1. **Require Status Checks**:

   - Enable "Require status checks to pass before merging"
   - Select the CI jobs: `security-audit`, `lint-and-format`, `test`, `build-cross-platform`
   - Optionally require `semantic-pr` for conventional commit enforcement

1. **Require Reviews**:

   - Enable "Require a pull request review before merging"
   - Set "Required number of reviewers before merging" to 1
   - Enable "Dismiss stale PR approvals when new commits are pushed"

## CODEOWNERS Integration

The `.github/CODEOWNERS` file is configured to:

- Assign `@UncleSp1d3r` as the default reviewer
- Ensure all code changes are reviewed
- Work seamlessly with CodeRabbit

## Verification

To verify the configuration:

1. Create a test PR
1. Verify that CodeRabbit provides reviews
1. Confirm that GitHub Copilot auto-reviews are disabled
1. Check that required status checks are enforced

## Notes

- CodeRabbit provides more detailed, context-aware code reviews
- It integrates well with conventional commits and semantic versioning
- The configuration respects the user preference for CodeRabbit over Copilot
- Manual review by `@UncleSp1d3r` is still required as the repository maintainer
