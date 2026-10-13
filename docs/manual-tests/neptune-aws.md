# Manual test: Amazon Neptune on AWS

CI has no real Neptune (ADR-0026). Its tests use a fake Neptune that checks SigV4 the way the real one does.
Run this checklist against a real cluster before a release that changes the Neptune provider.

## You need
- A Neptune cluster with **IAM database authentication** turned on, and a reader endpoint if you have one.
- An identity allowed to call `neptune-db:connect`, `neptune-db:ReadDataViaQuery` and
  `neptune-db:GetEngineStatus`. Statistics need `neptune-db:GetStatisticsStatus`, and profile/explain need
  `neptune-db:ReadDataViaQuery`.
- Network access to the cluster. Use one of:
  - a host inside the VPC;
  - an SSH tunnel: `ssh -N -L 8182:<cluster-endpoint>:8182 ec2-user@<bastion>`;
  - an SSM tunnel: `aws ssm start-session --target <instance> --document-name AWS-StartPortForwardingSessionToRemoteHost --parameters host=<cluster-endpoint>,portNumber=8182,localPortNumber=8182`.
- An AWS profile. Test at least one SSO profile and one access-key or assume-role profile.

## Checklist
Record the engine version, the date and the outcome of each step.

1. **Profiles listed.** `gremliner-cli connections aws-profiles` lists the profiles in `~/.aws/config` and
   `~/.aws/credentials`, with SSO profiles marked. In the app, New connection → Amazon Neptune shows the
   same profiles as chips.
2. **Region inferred.** Typing or pasting the cluster endpoint fills in the region.
3. **Direct connection (inside the VPC).** Choose a profile with IAM on, then Test connection. Expect
   "Connected in N ms · Neptune <engine> · TinkerPop <version>".
4. **Through a tunnel.** Set Tunnel address to `localhost:8182`, keep the host as the cluster endpoint, and
   turn on "Accept invalid certificates" only if the tunnel's certificate does not match. Test connection
   must succeed. The CLI equivalent is
   `gremliner-cli connections add --name nep --url <endpoint> --profile <p> --tunnel localhost:8182`,
   then `gremliner-cli connections test nep`.
5. **Query.** `gremliner-cli schema --conn nep` prints the labels (from the statistics summary when it is
   enabled). In the app, `g.V().limit(5).valueMap()` streams rows into the table.
6. **Expired SSO.** Run `aws sso logout`, then Test connection. Expect "The AWS SSO session for profile
   "<p>" has expired..." with `aws sso login --profile <p>`. Log back in and Test again; it succeeds
   without restarting the app.
7. **Credential refresh.** Leave a connection open longer than the credentials' lifetime (an assumed role
   with a 15-minute session), let the socket drop (sleep the laptop, or restart the tunnel), and run a query.
   It reconnects without prompting.
8. **IAM mismatch.** Turn IAM off on an IAM-enabled cluster. Expect "refused the connection (HTTP 403)"
   with a suggestion that mentions IAM.
9. **Schema summary.** With statistics enabled on the cluster, connecting shows the schema tree almost
   immediately (from `/propertygraph/statistics/summary`). With statistics disabled, the tree is filled by
   sampling instead.
10. **Restrictions.** In an editor tab on the Neptune connection, `g.V().map{ it.get() }` is underlined with
    "Neptune does not support lambdas". `g.io('x').read()` is flagged as unsupported.
11. **Assistant.** Asking the prompt bar for something lambda-like produces a lambda-free query, and the
    assistant does not suggest `io()` or `pageRank()`.
