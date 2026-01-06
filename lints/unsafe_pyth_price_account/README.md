# `unsafe_pyth_price_account`

### What it does
Detects unsafe usage of Pyth PriceUpdateV2 accounts where a program relies on `feed_id` and `max_age` validation but does not enforce canonical price sources or monotonic publish times.

### Why is this bad?
Using non-canonical Pyth price feeds or not enforcing monotonic publish times can allow attackers to provide stale or manipulated price data, leading to incorrect pricing decisions and potential fund loss.

