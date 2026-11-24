# Your first session on Termux

Start in a disposable directory so you can inspect every change. After [installing Blue Dragon](../README.md#quick-start), type `/exit` if the terminal interface is open. The following commands run in the Termux shell:

```sh
demo=$(mktemp -d "$HOME/blue-dragon-demo.XXXXXX")
cd "$demo"
cat > cart.js <<'JS'
export function total(items) {
  return items.reduce((sum, item) => sum + item.price, 0);
}
JS
blue-dragon map
```

Look for `cart.js` under `sample_files` and `JavaScript` under `languages`. This step runs locally. The file deliberately starts with a single function so you can check the answer yourself.

## Ask about the code

```sh
blue-dragon --provider free tui
```

In the session, ask:

> Read cart.js. What does total([]) return? Quote the part of the function that determines the result.

The initial value passed to `reduce` is zero. With an empty array, the callback never runs and the function returns that initial value. The hosted provider needs a network connection for this part.

## Review one change

> Update total so each item's price is multiplied by its quantity. If quantity is missing, treat it as 1. Keep a quantity of 0 as 0.

Inspect the proposed diff in the terminal. A fallback such as `item.quantity || 1` would charge for an item with quantity zero. A nullish fallback such as `item.quantity ?? 1` preserves zero. Approve the edit once it matches the requested behavior.

Type `/exit`, then run `cat cart.js` in the same directory to see exactly what was saved. For another session in your own project, change into that project and run `blue-dragon --provider free tui`.
