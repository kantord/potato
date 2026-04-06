describe("Live Grep App", () => {
  it("loads and shows the title", async () => {
    const heading = await $("h1");
    await heading.waitForDisplayed({ timeout: 10000 });
    expect(await heading.getText()).toBe("Live Grep");
  });

  it("shows subtitle mentioning Alice", async () => {
    const subtitle = await $("em");
    expect(await subtitle.getText()).toContain("Alice");
  });

  it("has a search input", async () => {
    const input = await $("input[name='query']");
    expect(await input.isDisplayed()).toBe(true);
  });

  it("finds results for 'rabbit'", async () => {
    const input = await $("input[name='query']");
    await input.setValue("rabbit");

    await browser.waitUntil(
      async () => {
        const results = await $("#results");
        const text = await results.getText();
        return text.toLowerCase().includes("rabbit");
      },
      { timeout: 10000, timeoutMsg: "expected results to contain 'rabbit'" }
    );
  });

  it("finds results for 'queen'", async () => {
    const input = await $("input[name='query']");
    await input.setValue("queen");

    await browser.waitUntil(
      async () => {
        const results = await $("#results");
        const text = await results.getText();
        return text.toLowerCase().includes("queen");
      },
      { timeout: 10000, timeoutMsg: "expected results to contain 'queen'" }
    );
  });

  it("shows no results for nonsense query", async () => {
    const input = await $("input[name='query']");
    await input.setValue("xyzzyflurbo");

    await browser.waitUntil(
      async () => {
        const results = await $("#results");
        const text = await results.getText();
        return text.length > 0 && !text.includes("Searching");
      },
      { timeout: 10000, timeoutMsg: "expected results to update" }
    );

    const results = await $("#results");
    const text = await results.getText();
    expect(text.toLowerCase()).not.toContain("xyzzyflurbo");
  });

  it("is case insensitive", async () => {
    const input = await $("input[name='query']");
    await input.setValue("ALICE");

    await browser.waitUntil(
      async () => {
        const results = await $("#results");
        const text = await results.getText();
        return text.toLowerCase().includes("alice");
      },
      { timeout: 10000, timeoutMsg: "expected case-insensitive results for 'ALICE'" }
    );
  });
});
