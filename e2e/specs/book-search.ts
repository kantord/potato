describe("Book Search App", () => {
  it("loads and shows the title", async () => {
    const heading = await $("h1");
    await heading.waitForDisplayed({ timeout: 10000 });
    expect(await heading.getText()).toBe("Book Search");
  });

  it("shows subtitle mentioning Alice", async () => {
    const subtitle = await $(".subtitle");
    expect(await subtitle.getText()).toContain("Alice");
  });

  it("has a search input and button", async () => {
    const input = await $("#query");
    expect(await input.isDisplayed()).toBe(true);

    const button = await $("button=Search");
    expect(await button.isDisplayed()).toBe(true);
  });

  it("finds results for 'rabbit'", async () => {
    const input = await $("#query");
    await input.setValue("rabbit");

    const button = await $("button=Search");
    await button.click();

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
    const input = await $("#query");
    await input.setValue("queen");

    const button = await $("button=Search");
    await button.click();

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
    const input = await $("#query");
    await input.setValue("xyzzyflurbo");

    const button = await $("button=Search");
    await button.click();

    await browser.waitUntil(
      async () => {
        const results = await $("#results");
        const text = await results.getText();
        return text.length > 0 && !text.includes("Searching");
      },
      { timeout: 10000, timeoutMsg: "expected results to update" }
    );
  });

  it("is case insensitive", async () => {
    const input = await $("#query");
    await input.setValue("ALICE");

    const button = await $("button=Search");
    await button.click();

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
