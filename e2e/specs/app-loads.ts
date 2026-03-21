describe("Potato Apps", () => {
  it("hello-world app loads and shows title", async () => {
    // The app navigates to potato://localhost which serves index.html
    // Wait for the React app to render
    const heading = await $("h1");
    await heading.waitForDisplayed({ timeout: 10000 });
    const text = await heading.getText();
    expect(text).toBe("Hello from Potato!");
  });

  it("hello-world app has calculator buttons", async () => {
    const addButton = await $("button=Add");
    await addButton.waitForDisplayed({ timeout: 5000 });
    expect(await addButton.isDisplayed()).toBe(true);

    const multiplyButton = await $("button=Multiply");
    expect(await multiplyButton.isDisplayed()).toBe(true);
  });

  it("hello-world calculator works", async () => {
    // Set input values
    const inputs = await $$("input[type='number']");
    await inputs[0].setValue("7");
    await inputs[1].setValue("3");

    // Click Add
    const addButton = await $("button=Add");
    await addButton.click();

    // Wait for result to appear with the calculated value
    await browser.waitUntil(
      async () => {
        const el = await $("p");
        if (!(await el.isExisting())) return false;
        const t = await el.getText();
        return t.includes("10");
      },
      { timeout: 10000, timeoutMsg: "expected result to contain 10" }
    );
  });
});
