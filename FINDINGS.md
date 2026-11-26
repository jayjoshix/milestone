# Making Binary Milestones Deterministic Before They Trade

> “Will Acme raise $5M this year?” appears binary.
>
> It is not binary until the market defines whether signing, announcement, or receipt of funds controls settlement.

When real capital trades on a milestone headline, two reasonable resolvers looking at the same documents can still arrive at opposite `YES`/`NO` outcomes unless the underlying resolution rules are explicit.

Below are three concrete examples from the resolution-design surface.

---

## Case 1 — Fundraising

Headline:

```text
Will Acme raise $5M before Dec 31?
```

Counterexample:

```text
SAFE signed Dec 29.
Funds received Jan 3.
```

Possible results:

```text
YES — signing controls
NO  — receipt controls
```

Missing rule:

```text
fundraising.completion_event
```

---

## Case 2 — ARR

Headline:

```text
Will Acme reach $1M ARR?
```

Counterexample:

```text
$950k recurring subscriptions
+$100k annual professional services
```

Possible results:

```text
YES — services included
NO  — recurring software only
```

Missing rule:

```text
arr.included_revenue
```

---

## Case 3 — Acquisition

Headline:

```text
Will Acme be acquired for at least $100M?
```

Counterexample:

```text
$80M cash at close
+$30M contingent earnout
```

Possible results:

```text
YES — total announced consideration
NO  — unconditional consideration only
```

Missing rule:

```text
acquisition.consideration_policy
```

---

> The implementation goal is not to decide which policy is correct.
>
> It is to ensure the policy is explicit before the market becomes economically consequential.
