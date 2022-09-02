from selenium import webdriver
from selenium.webdriver.common.keys import Keys
from selenium.webdriver.common.by import By

driver = webdriver.Firefox()

url = "https://www.xcontest.org/world/en/"

print("===== Enter login data =====")
username = input("Username: ")
password = input("Password: ")
print("===== Getting flights =====")

# open webpage
driver.get(url)
# wait to load page
driver.implicitly_wait(3)
# enter username & password for login
login_username = driver.find_element(By.ID, "login-username")
login_username.send_keys(username)
login_password = driver.find_element(By.ID, "login-password")
login_password.send_keys(password)
login_password.send_keys(Keys.ENTER)
# go to my_flights
my_flights = driver.find_element(By.XPATH, "")
# download all flights

print("===== Finished =====")