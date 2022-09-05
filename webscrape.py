from time import sleep
from selenium import webdriver
from selenium.webdriver.common.keys import Keys
from selenium.webdriver.common.by import By
import zipfile
import os
from datetime import date
import time

driver = webdriver.Firefox()


def check_number_of_files(path):
    directory = os.listdir(path)
    number_of_files = len(directory)
    return number_of_files


# login data
print("===== Enter login data =====")
username = input("Username: ")
password = input("Password: ")
print("===== Getting flights =====")

# variables
url = "https://www.xcontest.org/world/en/"
download_path = os.path.join(os.environ['HOME'], "Downloads/")
file_name = f"igc.{username}.world{str(date.today().year)}.zip"
file_path = download_path + file_name

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
my_flights = driver.find_element(By.LINK_TEXT, "My flights")
my_flights.click()
# download all flights
print("===== Downloading... =====")
download_flights = driver.find_element(By.LINK_TEXT, "IGC")
driver.execute_script("arguments[0].click();", download_flights)
# wait until download completed
# 1) firefox creates the end file and a tmp file for the download
# 2) count number of files before download
# 3) check until only 1 new file instead of 2 -> download finished
dl_completed = False
# TODO fix timeout code
t_start = time.time()
initial_number = check_number_of_files(download_path)
while not dl_completed or (time.time() - t_start) < 20:
    sleep(1)
    # check if file exists
    if os.path.isfile(file_path) and check_number_of_files(download_path) == initial_number + 1:
        dl_completed = True
        break
print("===== Finished download =====")
# extract flights
print("===== Extracting... =====")
with zipfile.ZipFile(file_path, 'r') as zip_ref:
    zip_ref.extractall(f"{download_path}/{username}_flights")

driver.quit()
print("===== Finished =====")
