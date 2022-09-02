from selenium import webdriver
from selenium.webdriver.common.keys import Keys
from selenium.webdriver.common.by import By
from selenium.webdriver.support.ui import WebDriverWait
from selenium.webdriver.support import expected_conditions as EC
import zipfile
import os

driver = webdriver.Firefox()

url = "https://www.xcontest.org/world/en/"
download_path = os.path.join(os.environ['HOME'], "Downloads/")

print("===== Enter login data =====")
username = input("Username: ")
password = input("Password: ")
print("===== Getting flights =====")

# open webpage
driver.get(url)
# wait to load page
driver.implicitly_wait(10)
# enter username & password for login
login_username = driver.find_element(By.ID, "login-username")
login_username.send_keys(username)
login_password = driver.find_element(By.ID, "login-password")
login_password.send_keys(password)
login_password.send_keys(Keys.ENTER)
# go to my_flights
my_flights = driver.find_element(By.LINK_TEXT, "My flights")
my_flights.click()
# download all flights
download_flights = WebDriverWait(driver, 10).until(
    EC.element_to_be_clickable((By.LINK_TEXT, "IGC"))
)
# extract flights
with zipfile.ZipFile(download_path+"*.zip", 'r') as zip_ref:
    zip_ref.extractall(download_path)

#driver.quit()
print("===== Finished =====")