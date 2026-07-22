# How to locally test application-services components on HNT / Desktop

> This guide explains how to build and test **HNT against a local Application Services** checkout.

---

## At a glance

**Goal:** Build a local Firefox Desktop against a local Application Services.

**Current workflow (recommended):**

1. Verify the local build of `A-S` is ready for a desktop build.
2. Move the A-S components folder in M-C/Firefox to a temporary rename.
3. Create a symlink between the A-S and M-C components folders.
4. Generate uniffi bindings.
5. Run and build.
6. Cleanup of symlink and temporary renames.

---

## Prerequisites

1. Ensure you have a regular [build of application-services working](../building.md).
2. Ensure you have a regular [build of firefox from mozilla-central](https://firefox-source-docs.mozilla.org/setup/index.html#for-firefox-desktop) testable with `./mach build` and `./mach run`.

---

## Step 1 — Verify the local build of A-S is ready for a desktop build

From the root of your `application-services` checkout, execute:

```bash
./libs/verify-desktop-environment.sh
```

This will check for environment variables. If it provides any instruction on environment variables to set, follow the instructions until it passes.


## Step 2 - Move the A-S components folder in M-C/Firefox to a temporary rename

We will be temporarily replacing the components in the `application-services` repository in `mozilla-central` with a symlink that points to our local `application-services` build. To conserve the old folder, we temporarily rename it. From the **mozilla-central** root.

```bash
mv third_party/application-services/components third_party/application-services/components-tmp
```

## Step 3 - Create a symlink between the A-S and M-C components folders.

Now, the former `components` path should have a symlink to the local `application-services` components. Assuming `application-services` is in the same folder as your `mozilla-central` checkout, you can run (from the **mozilla-central** root):

```bash
ln -s $(realpath ../application-services/components) third_party/application-services/components
```

## Step 4 - Generate uniffi bindings.

You may need to regenerate uniffi bindings, as if you vendored new `A-S` code. From the mozilla-central root:

```bash
./mach uniffi generate
```

## Step 5 - Run and build!

Now that `components` will read from your local build, you can build, run, and test. From your local m-c checkout, run:

```bash
./mach build
```

And if so desired:

```bash
./mach run
```

## Step 6 - Cleanup

After completing your tests, you should revert your files and symlinks to ensure `m-c` continues to behave as expected:

```bash
unlink third_party/application-services/components
mv third_party/application-services/components-tmp third_party/application-services/components
```

# Automated testing

You can also automate this process by running the Desktop smoke test found at `automation/build_against_hnt.py`. You can see more detailed instructions about this [here](./smoke-testing-app-services.md).
