/**
 * This is just an example, so you can safely delete all default props below
 * @param element - The HTML button element to attach the counter functionality to
 */
export function setupCounter(element: HTMLButtonElement) {
  let counter = 0
  const setCounter = (count: number) => {
    counter = count
    element.innerHTML = `Count is ${counter}`
  }
  element.addEventListener('click', () => setCounter(counter + 1))
  setCounter(0)
}
