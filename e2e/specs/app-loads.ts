describe("SpudKit Apps", () => {
  it("hello-world app loads and shows title", async () => {
    const heading = await $("h1");
    await heading.waitForDisplayed({ timeout: 10000 });
    const text = await heading.getText();
    expect(text).toBe("Hello from SpudKit!");
  });

  it("hello-world app has calculator buttons", async () => {
    const addButton = await $("button=Add");
    await addButton.waitForDisplayed({ timeout: 5000 });
    expect(await addButton.isDisplayed()).toBe(true);

    const multiplyButton = await $("button=Multiply");
    expect(await multiplyButton.isDisplayed()).toBe(true);
  });

  it("hello-world calculator adds correctly", async () => {
    const inputs = await $$("input[type='number']");
    await inputs[0].setValue("7");
    await inputs[1].setValue("3");

    const addButton = await $("button=Add");
    await addButton.click();

    await browser.waitUntil(
      async () => {
        const el = await $("p");
        if (!(await el.isExisting())) return false;
        const t = await el.getText();
        return t.includes("10");
      },
      { timeout: 10000, timeoutMsg: "expected add result to contain 10" }
    );

    const result = await $("p");
    expect(await result.getText()).toContain("Result: 10");
  });

  it("hello-world calculator multiplies correctly", async () => {
    const inputs = await $$("input[type='number']");
    await inputs[0].setValue("6");
    await inputs[1].setValue("4");

    const multiplyButton = await $("button=Multiply");
    await multiplyButton.click();

    await browser.waitUntil(
      async () => {
        const el = await $("p");
        if (!(await el.isExisting())) return false;
        const t = await el.getText();
        return t.includes("24");
      },
      { timeout: 10000, timeoutMsg: "expected multiply result to contain 24" }
    );

    const result = await $("p");
    expect(await result.getText()).toContain("Result: 24");
  });

  it("hello-world calculator handles decimals", async () => {
    const inputs = await $$("input[type='number']");
    await inputs[0].setValue("2.5");
    await inputs[1].setValue("4");

    const addButton = await $("button=Add");
    await addButton.click();

    await browser.waitUntil(
      async () => {
        const el = await $("p");
        if (!(await el.isExisting())) return false;
        const t = await el.getText();
        return t.includes("6.5");
      },
      { timeout: 10000, timeoutMsg: "expected decimal result to contain 6.5" }
    );

    const result = await $("p");
    expect(await result.getText()).toContain("Result: 6.5");
  });

  it("hello-world calculator handles negative numbers", async () => {
    const inputs = await $$("input[type='number']");
    await inputs[0].setValue("-3");
    await inputs[1].setValue("5");

    const multiplyButton = await $("button=Multiply");
    await multiplyButton.click();

    await browser.waitUntil(
      async () => {
        const el = await $("p");
        if (!(await el.isExisting())) return false;
        const t = await el.getText();
        return t.includes("-15");
      },
      { timeout: 10000, timeoutMsg: "expected negative result to contain -15" }
    );

    const result = await $("p");
    expect(await result.getText()).toContain("Result: -15");
  });
});
